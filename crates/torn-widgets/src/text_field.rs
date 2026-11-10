use std::time::{Duration, Instant};

use torn_core::{
    Color, Constraints, FocusChanged, InputEvent, Insets, Key, Modifiers, NamedKey, Point,
    PointerButton, Rect, Size, TextPreedit, WheelDelta,
};
use torn_render::{PaintContext, TextLayout, TextStyle};
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    StyleTarget, TssStyle, UiEnvironment, Widget,
};

use crate::text_editor::TextEditor;

const DEFAULT_WIDTH: f32 = 120.0;
const HORIZONTAL_PADDING: f32 = 8.0;
const VERTICAL_PADDING: f32 = 6.0;
const BORDER_WIDTH: f32 = 1.0;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(400);

/// A single-line editable text control.
///
/// The control supports grapheme-cluster caret movement, shift and pointer
/// selection, clipboard operations, undo and redo, input-method preedit, and
/// horizontal scrolling when the value is wider than the control. Text is
/// committed through [`InputEvent::TextInput`], which keeps keyboard layout and
/// input-method handling in the platform adapter.
pub struct TextField {
    editor: TextEditor,
    changed: Signal<String>,
    focused: bool,
    preedit: Option<TextPreedit>,
    text_layout: TextLayout,
    text_style: TextStyle,
    padding: Insets,
    viewport: Size,
    scroll_x: f32,
    dragging: bool,
    last_click: Option<(Instant, Point)>,
}

impl TextField {
    /// Creates an empty text field.
    #[must_use]
    pub fn new() -> Self {
        Self::with_text("")
    }

    /// Creates a text field with `text` as its initial value.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            editor: TextEditor::with_text(false, text),
            changed: Signal::new(),
            focused: false,
            preedit: None,
            text_layout: empty_layout(),
            text_style: TextStyle::new(1.0, Color::BLACK),
            padding: default_padding(),
            viewport: Size::ZERO,
            scroll_x: 0.0,
            dragging: false,
            last_click: None,
        }
    }

    /// Returns the current UTF-8 text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.editor.text()
    }

    /// Replaces the current text and places the caret at its end.
    ///
    /// This does not emit [`Self::changed`]; use the signal for edits initiated
    /// by the user.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.editor.set_text(text);
        self.preedit = None;
        self.scroll_x = 0.0;
    }

    /// Returns the current caret position in Unicode scalar values.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.editor.text()[..self.editor.focus()].chars().count()
    }

    /// Sets the caret position in Unicode scalar values, clamping it to the text.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.editor
            .set_caret(byte_index(self.editor.text(), cursor));
    }

    /// Returns the current UTF-8 selection, if the user selected text.
    #[must_use]
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.editor.selection()
    }

    /// Selects a UTF-8 byte range, clamped to char boundaries.
    pub fn select(&mut self, anchor: usize, focus: usize) {
        self.editor.select(anchor, focus);
    }

    /// Returns a signal emitted after user input changes the text.
    #[must_use]
    pub fn changed(&self) -> Signal<String> {
        self.changed.clone()
    }

    fn text_origin(&self) -> Point {
        Point::new(self.padding.left, self.padding.top)
    }

    fn display(&self) -> (String, usize) {
        let selection = self.editor.selection();
        let preedit = self
            .preedit
            .as_ref()
            .map_or("", |preedit| preedit.text.as_str());
        if preedit.is_empty() {
            return (self.editor.text().to_owned(), self.editor.focus());
        }
        let mut text = String::with_capacity(self.editor.text().len() + preedit.len());
        text.push_str(&self.editor.text()[..selection.start]);
        text.push_str(preedit);
        text.push_str(&self.editor.text()[selection.end..]);
        let preedit_caret = self.preedit.as_ref().map_or(preedit.len(), |preedit| {
            preedit
                .cursor
                .map_or(preedit.text.len(), |(_, end)| end)
                .min(preedit.text.len())
        });
        let caret = selection.start + preedit_caret;
        (text, caret)
    }

    fn relayout(&mut self, environment: &UiEnvironment, width: Option<f32>) {
        let (text, _) = self.display();
        self.text_layout = environment
            .text_shaper()
            .layout(&text, &self.text_style, width);
    }

    fn ensure_caret_visible(&mut self) {
        let caret = self.text_layout.caret_rect(self.display().1, 1.0).origin.x;
        let width = (self.viewport.width() - self.padding.horizontal()).max(1.0);
        if caret < self.scroll_x {
            self.scroll_x = caret;
        } else if caret + 1.0 > self.scroll_x + width {
            self.scroll_x = caret + 1.0 - width;
        }
        self.scroll_x = self.scroll_x.max(0.0);
    }

    fn hit_test_caret(&self, position: Point) -> usize {
        let origin = self.text_origin();
        self.text_layout.byte_at_point(Point::new(
            position.x - origin.x + self.scroll_x,
            position.y - origin.y,
        ))
    }

    fn select_word_at(&mut self, position: Point) {
        let byte = self.hit_test_caret(position);
        let text = self.editor.text();
        let mut start = byte;
        while start > 0 {
            let previous = self.editor.previous_grapheme(start);
            if text[previous..start]
                .chars()
                .all(|character| !is_word_character(character))
            {
                break;
            }
            start = previous;
        }
        let mut end = byte;
        while end < text.len() {
            let next = self.editor.next_grapheme(end);
            if text[end..next]
                .chars()
                .all(|character| !is_word_character(character))
            {
                break;
            }
            end = next;
        }
        self.editor.select(start, end);
    }

    /// Applies `edit`, notifying listeners when the text changed.
    fn apply_edit(
        &mut self,
        context: &mut EventContext<'_>,
        edit: impl FnOnce(&mut TextEditor) -> bool,
    ) -> bool {
        let before = self.editor.text().to_owned();
        if !edit(&mut self.editor) {
            return false;
        }
        if before == self.editor.text() {
            context.request_redraw();
            return true;
        }
        self.changed.emit(&self.editor.text().to_owned());
        context.request_layout();
        true
    }

    fn copy(&mut self, context: &EventContext<'_>) -> bool {
        let Some(selection) = self.editor.selected_text() else {
            return false;
        };
        context.write_clipboard(selection);
        true
    }

    fn cut(&mut self, context: &mut EventContext<'_>) -> bool {
        if !self.copy(context) {
            return false;
        }
        self.apply_edit(context, TextEditor::delete_selection);
        true
    }

    fn paste(&mut self, context: &mut EventContext<'_>) -> bool {
        let Some(text) = context.read_clipboard() else {
            return false;
        };
        self.apply_edit(context, |editor| editor.insert(&text));
        true
    }
}

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextField {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let font = environment.font_for(&style);
        self.text_style = TextStyle::new_with_font(
            font.clone(),
            style.foreground_or(environment.theme().foreground()),
        );
        self.padding = style.padding_or(default_padding());
        self.relayout(environment, None);
        let font_size = font.pixel_size(environment.logical_dpi());
        let width =
            (self.text_layout.size().width() + self.padding.horizontal()).max(DEFAULT_WIDTH);
        let height = (self.text_layout.size().height() + self.padding.vertical())
            .max(font_size + self.padding.vertical());
        let size = constraints.constrain(size(width, height));
        self.viewport = size;
        self.ensure_caret_visible();
        LayoutResult::new(size)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("text-field")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let theme = environment.theme();
        let radius = style.corner_radius_or(theme.corner_radius());
        let foreground = style.foreground_or(theme.foreground());
        let accent = style.accent_or(theme.accent());
        context.fill_rounded_rect(bounds, radius, style.background_or(theme.background()));
        let border = style.border.map_or_else(
            || {
                if self.focused {
                    accent
                } else {
                    theme.control_background()
                }
            },
            |border| border.color,
        );
        let border_width = style.border.map_or(BORDER_WIDTH, |border| border.width);
        context.stroke_rounded_rect(bounds, radius, border_width, border);

        let text_bounds = bounds.inset(self.padding);
        let origin = Point::new(text_bounds.origin.x - self.scroll_x, text_bounds.origin.y);
        context.with_clip(text_bounds, |context| {
            let layout = if self.text_layout.color() == foreground {
                self.text_layout.clone()
            } else {
                self.text_layout.clone().with_color(foreground)
            };
            if self.preedit.is_none() && self.editor.has_selection() {
                for rect in layout.selection_rects(self.editor.selection()) {
                    context.fill_rect(
                        Rect::new(
                            Point::new(rect.origin.x + origin.x, rect.origin.y + origin.y),
                            rect.size,
                        ),
                        accent.with_alpha(0.28),
                    );
                }
            }
            context.draw_text(layout.clone(), origin);
            if self.focused {
                let (_, caret) = self.display();
                let caret_rect = layout.caret_rect(caret, 1.0);
                context.fill_rect(
                    Rect::new(
                        Point::new(caret_rect.origin.x + origin.x, text_bounds.origin.y),
                        size(1.0, text_bounds.size.height().max(caret_rect.size.height())),
                    ),
                    accent,
                );
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) => {
                if pointer.button != Some(PointerButton::Primary) {
                    return EventStatus::Ignored;
                }
                context.request_focus();
                context.capture_pointer(pointer.pointer_id);
                let now = Instant::now();
                let double_click = self.last_click.is_some_and(|(time, last)| {
                    now.duration_since(time) <= DOUBLE_CLICK_INTERVAL
                        && (last.x - pointer.position.x).abs() <= 4.0
                        && (last.y - pointer.position.y).abs() <= 4.0
                });
                self.last_click = Some((now, pointer.position));
                if double_click {
                    self.select_word_at(pointer.position);
                } else if pointer.modifiers.contains(Modifiers::SHIFT) {
                    self.editor
                        .move_to(self.hit_test_caret(pointer.position), true);
                } else {
                    self.editor.set_caret(self.hit_test_caret(pointer.position));
                }
                self.dragging = true;
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) => {
                if !self.dragging || !pointer.buttons.contains_button(PointerButton::Primary) {
                    return EventStatus::Ignored;
                }
                self.editor
                    .move_to(self.hit_test_caret(pointer.position), true);
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) => {
                if !self.dragging {
                    return EventStatus::Ignored;
                }
                self.dragging = false;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer) => {
                self.dragging = false;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::TextInput(text) => {
                let had_preedit = self.preedit.take().is_some();
                self.apply_edit(context, |editor| editor.insert(text));
                if had_preedit {
                    context.request_layout();
                }
                EventStatus::Handled
            }
            InputEvent::TextPreedit(preedit) => {
                self.preedit = (!preedit.text.is_empty()).then(|| preedit.clone());
                context.request_layout();
                EventStatus::Handled
            }
            InputEvent::Wheel(wheel) => {
                let delta = match wheel.delta {
                    WheelDelta::Pixels(delta) => delta,
                    WheelDelta::Lines(delta) => Point::new(delta.x * 24.0, delta.y * 24.0),
                };
                let total_width = self.text_layout.size().width() + self.padding.horizontal();
                if total_width <= self.viewport.width() {
                    return EventStatus::Ignored;
                }
                self.scroll_x = (self.scroll_x - delta.y - delta.x).max(0.0);
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) => self.handle_key(context, key),
            InputEvent::FocusChanged(FocusChanged { focused }) => {
                self.focused = *focused == Some(context.current_target());
                if !self.focused {
                    self.dragging = false;
                    if self.preedit.take().is_some() {
                        context.request_layout();
                    }
                }
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::TextField)
            .with_value(self.editor.text())
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(
                AccessibilityActions::FOCUS
                    | AccessibilityActions::SET_VALUE
                    | AccessibilityActions::SELECT,
            );
        if self.editor.has_selection() {
            accessibility =
                accessibility.with_text_selection(self.editor.anchor(), self.editor.focus());
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::SetValue(value) => {
                self.set_text(value.clone());
                true
            }
            AccessibilityAction::Select => {
                self.editor.select_all();
                true
            }
            _ => false,
        }
    }
}

impl TextField {
    fn handle_key(
        &mut self,
        context: &mut EventContext<'_>,
        key: &torn_core::KeyEvent,
    ) -> EventStatus {
        let extend = key.modifiers.contains(Modifiers::SHIFT);
        let command =
            key.modifiers.contains(Modifiers::CONTROL) || key.modifiers.contains(Modifiers::META);
        match &key.key {
            Key::Named(NamedKey::Backspace) => {
                self.apply_edit(context, |editor| editor.backspace(command));
                EventStatus::Handled
            }
            Key::Named(NamedKey::Delete) => {
                self.apply_edit(context, |editor| editor.delete(command));
                EventStatus::Handled
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.editor.move_horizontal(false, command, extend);
                self.ensure_caret_visible();
                context.request_redraw();
                EventStatus::Handled
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.editor.move_horizontal(true, command, extend);
                self.ensure_caret_visible();
                context.request_redraw();
                EventStatus::Handled
            }
            Key::Named(NamedKey::Home) => {
                self.editor.move_to(0, extend);
                self.ensure_caret_visible();
                context.request_redraw();
                EventStatus::Handled
            }
            Key::Named(NamedKey::End) => {
                self.editor.move_to(self.editor.text().len(), extend);
                self.ensure_caret_visible();
                context.request_redraw();
                EventStatus::Handled
            }
            Key::Named(NamedKey::Enter | NamedKey::Tab) => EventStatus::Ignored,
            _ if command => {
                let handled = match &key.key {
                    Key::Character(character) if character.eq_ignore_ascii_case("a") => {
                        self.editor.select_all();
                        true
                    }
                    Key::Character(character) if character.eq_ignore_ascii_case("c") => {
                        self.copy(context)
                    }
                    Key::Character(character) if character.eq_ignore_ascii_case("x") => {
                        self.cut(context)
                    }
                    Key::Character(character) if character.eq_ignore_ascii_case("v") => {
                        self.paste(context)
                    }
                    Key::Character(character) if character.eq_ignore_ascii_case("z") => {
                        if extend {
                            self.apply_edit(context, TextEditor::redo)
                        } else {
                            self.apply_edit(context, TextEditor::undo)
                        }
                    }
                    Key::Character(character) if character.eq_ignore_ascii_case("y") => {
                        self.apply_edit(context, TextEditor::redo)
                    }
                    _ => false,
                };
                if handled {
                    self.ensure_caret_visible();
                    context.request_redraw();
                    EventStatus::Handled
                } else {
                    EventStatus::Ignored
                }
            }
            _ => EventStatus::Ignored,
        }
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn byte_index(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

fn default_padding() -> Insets {
    Insets::new(
        VERTICAL_PADDING,
        HORIZONTAL_PADDING,
        VERTICAL_PADDING,
        HORIZONTAL_PADDING,
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("text field sizes are non-negative and not NaN")
}

fn empty_layout() -> TextLayout {
    torn_render::FontdueTextShaper::ubuntu_light().layout(
        "",
        &TextStyle::new(1.0, Color::BLACK),
        None,
    )
}
