//! Shared text-editing model for single- and multi-line controls.
//!
//! The model stores UTF-8 byte offsets, moves on grapheme-cluster boundaries,
//! and keeps independent undo and redo stacks. Widgets add layout-dependent
//! behavior such as line navigation, hit testing, and scrolling on top.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

const UNDO_LIMIT: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    text: String,
    anchor: usize,
    focus: usize,
}

/// A selection or caret expressed as UTF-8 byte offsets.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextEditor {
    text: String,
    anchor: usize,
    focus: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    multiline: bool,
}

impl TextEditor {
    pub(crate) fn with_text(multiline: bool, text: impl Into<String>) -> Self {
        let text = normalize(multiline, &text.into());
        let focus = text.len();
        Self {
            text,
            anchor: focus,
            focus,
            undo: Vec::new(),
            redo: Vec::new(),
            multiline,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn set_text(&mut self, text: impl Into<String>) {
        self.text = normalize(self.multiline, &text.into());
        self.anchor = self.text.len();
        self.focus = self.anchor;
        self.undo.clear();
        self.redo.clear();
    }

    pub(crate) const fn anchor(&self) -> usize {
        self.anchor
    }

    pub(crate) const fn focus(&self) -> usize {
        self.focus
    }

    pub(crate) fn selection(&self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.anchor != self.focus
    }

    pub(crate) fn selected_text(&self) -> Option<&str> {
        let selection = self.selection();
        (!selection.is_empty()).then(|| &self.text[selection])
    }

    /// Moves both ends of the selection to `byte`, clamped to a char boundary.
    pub(crate) fn set_caret(&mut self, byte: usize) {
        let byte = self.clamp(byte);
        self.anchor = byte;
        self.focus = byte;
    }

    pub(crate) fn select(&mut self, anchor: usize, focus: usize) {
        self.anchor = self.clamp(anchor);
        self.focus = self.clamp(focus);
    }

    pub(crate) fn select_all(&mut self) {
        self.anchor = 0;
        self.focus = self.text.len();
    }

    pub(crate) fn move_horizontal(&mut self, forward: bool, by_word: bool, extend: bool) {
        let target = if by_word {
            if forward {
                self.next_word(self.focus)
            } else {
                self.previous_word(self.focus)
            }
        } else if forward {
            self.next_grapheme(self.focus)
        } else {
            self.previous_grapheme(self.focus)
        };
        self.move_to(target, extend);
    }

    pub(crate) fn move_to(&mut self, target: usize, extend: bool) {
        let target = self.clamp(target);
        if extend {
            self.focus = target;
        } else {
            self.anchor = target;
            self.focus = target;
        }
    }

    pub(crate) fn move_vertical(&mut self, target: usize, extend: bool) {
        self.move_to(target, extend);
    }

    pub(crate) fn previous_grapheme(&self, byte: usize) -> usize {
        let byte = self.clamp(byte);
        self.text[..byte]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    pub(crate) fn next_grapheme(&self, byte: usize) -> usize {
        let byte = self.clamp(byte);
        self.text[byte..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.text.len(), |(offset, _)| byte + offset)
    }

    fn previous_word(&self, byte: usize) -> usize {
        let mut index = self.clamp(byte);
        while index > 0 {
            let previous = self.previous_grapheme(index);
            let grapheme = &self.text[previous..index];
            if !grapheme.chars().all(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        while index > 0 {
            let previous = self.previous_grapheme(index);
            let grapheme = &self.text[previous..index];
            if grapheme.chars().all(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        index
    }

    fn next_word(&self, byte: usize) -> usize {
        let mut index = self.clamp(byte);
        while index < self.text.len() {
            let next = self.next_grapheme(index);
            let grapheme = &self.text[index..next];
            if grapheme.chars().all(char::is_whitespace) {
                break;
            }
            index = next;
        }
        while index < self.text.len() {
            let next = self.next_grapheme(index);
            let grapheme = &self.text[index..next];
            if !grapheme.chars().all(char::is_whitespace) {
                break;
            }
            index = next;
        }
        index
    }

    /// Replaces the selection with `input`.
    pub(crate) fn insert(&mut self, input: &str) -> bool {
        let input = normalize(self.multiline, input);
        if input.is_empty() && !self.has_selection() {
            return false;
        }
        self.save_undo();
        let selection = self.selection();
        self.text.replace_range(selection.clone(), &input);
        let caret = selection.start + input.len();
        self.anchor = caret;
        self.focus = caret;
        self.redo.clear();
        true
    }

    /// Deletes the selection, or the grapheme before the caret.
    pub(crate) fn backspace(&mut self, by_word: bool) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.delete_selection();
        }
        if self.focus == 0 {
            return false;
        }
        let start = if by_word {
            self.previous_word(self.focus)
        } else {
            self.previous_grapheme(self.focus)
        };
        self.replace_range(start..self.focus)
    }

    /// Deletes the selection, or the grapheme after the caret.
    pub(crate) fn delete(&mut self, by_word: bool) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.delete_selection();
        }
        if self.focus >= self.text.len() {
            return false;
        }
        let end = if by_word {
            self.next_word(self.focus)
        } else {
            self.next_grapheme(self.focus)
        };
        self.replace_range(self.focus..end)
    }

    pub(crate) fn delete_selection(&mut self) -> bool {
        let selection = self.selection();
        if selection.is_empty() {
            return false;
        }
        self.replace_range(selection)
    }

    fn replace_range(&mut self, range: Range<usize>) -> bool {
        self.save_undo();
        self.text.replace_range(range.clone(), "");
        self.anchor = range.start;
        self.focus = range.start;
        self.redo.clear();
        true
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(&snapshot);
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(&snapshot);
        true
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            focus: self.focus,
        }
    }

    fn save_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    fn restore(&mut self, snapshot: &Snapshot) {
        self.text.clone_from(&snapshot.text);
        self.anchor = snapshot.anchor.min(self.text.len());
        self.focus = snapshot.focus.min(self.text.len());
    }

    fn clamp(&self, byte: usize) -> usize {
        let mut byte = byte.min(self.text.len());
        while byte > 0 && !self.text.is_char_boundary(byte) {
            byte -= 1;
        }
        byte
    }
}

fn normalize(multiline: bool, text: &str) -> String {
    if multiline {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.chars()
            .filter(|character| !matches!(character, '\r' | '\n'))
            .collect()
    }
}
