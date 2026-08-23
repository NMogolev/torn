use torn_core::{
    Color, Constraints, FocusChanged, InputEvent, Insets, Key, NamedKey, Point, Rect, Size,
};
use torn_render::{PaintContext, TextLayout, TextStyle};
use torn_ui::{
    EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal, UiEnvironment,
    Widget,
};

const DEFAULT_WIDTH: f32 = 120.0;
const HORIZONTAL_PADDING: f32 = 8.0;
const VERTICAL_PADDING: f32 = 6.0;
const BORDER_WIDTH: f32 = 1.0;

/// A single-line editable text control.
///
/// Text is committed through [`InputEvent::TextInput`], which keeps keyboard
/// layout and input-method handling in the platform adapter. The control emits
/// [`Self::changed`] whenever its value changes as a result of user input.
pub struct TextField {
    text: String,
    cursor: usize,
    changed: Signal<String>,
    focused: bool,
    text_layout: TextLayout,
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
        let text = single_line(&text.into());
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            changed: Signal::new(),
            focused: false,
            text_layout: empty_layout(),
        }
    }

    /// Returns the current UTF-8 text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the current text and places the caret at its end.
    ///
    /// This does not emit [`Self::changed`]; use the signal for edits initiated
    /// by the user.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = single_line(&text.into());
        self.cursor = self.text.chars().count();
    }

    /// Returns the current caret position in Unicode scalar values.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Sets the caret position, clamping it to the current text length.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.text.chars().count());
    }

    /// Returns a signal emitted after user input changes the text.
    #[must_use]
    pub fn changed(&self) -> Signal<String> {
        self.changed.clone()
    }

    fn replace_text(&mut self, text: String, context: &mut EventContext<'_>) {
        if text == self.text {
            return;
        }
        self.text = text;
        self.cursor = self.cursor.min(self.text.chars().count());
        self.changed.emit(&self.text);
        context.request_layout();
    }

    fn insert(&mut self, input: &str, context: &mut EventContext<'_>) {
        let input = single_line(input);
        if input.is_empty() {
            return;
        }
        let byte = byte_index(&self.text, self.cursor);
        let mut text = self.text.clone();
        text.insert_str(byte, &input);
        self.cursor += input.chars().count();
        self.replace_text(text, context);
    }

    fn backspace(&mut self, context: &mut EventContext<'_>) {
        if self.cursor == 0 {
            return;
        }
        let start = byte_index(&self.text, self.cursor - 1);
        let end = byte_index(&self.text, self.cursor);
        let mut text = self.text.clone();
        text.replace_range(start..end, "");
        self.cursor -= 1;
        self.replace_text(text, context);
    }

    fn delete(&mut self, context: &mut EventContext<'_>) {
        if self.cursor == self.text.chars().count() {
            return;
        }
        let start = byte_index(&self.text, self.cursor);
        let end = byte_index(&self.text, self.cursor + 1);
        let mut text = self.text.clone();
        text.replace_range(start..end, "");
        self.replace_text(text, context);
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
        let theme = context.environment().theme();
        self.text_layout = context.environment().text_shaper().layout(
            &self.text,
            &TextStyle::new(theme.font_size(), theme.foreground()),
            None,
        );
        let padding = padding();
        let width = (self.text_layout.size().width() + padding.horizontal()).max(DEFAULT_WIDTH);
        let height = (self.text_layout.size().height() + padding.vertical())
            .max(theme.font_size() + padding.vertical());
        LayoutResult::new(constraints.constrain(size(width, height)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let radius = theme.corner_radius();
        context.fill_rounded_rect(bounds, radius, theme.background());
        let border = if self.focused {
            theme.accent()
        } else {
            theme.button_background()
        };
        context.stroke_rounded_rect(bounds, radius, BORDER_WIDTH, border);

        let text_bounds = bounds.inset(padding());
        context.with_clip(text_bounds, |context| {
            context.draw_text(self.text_layout.clone(), text_bounds.origin);
            if self.focused {
                let prefix = &self.text[..byte_index(&self.text, self.cursor)];
                let prefix_layout = environment.text_shaper().layout(
                    prefix,
                    &TextStyle::new(theme.font_size(), theme.foreground()),
                    None,
                );
                let caret_height = text_bounds.size.height().max(theme.font_size());
                context.fill_rect(
                    Rect::new(
                        Point::new(
                            text_bounds.origin.x + prefix_layout.size().width(),
                            text_bounds.origin.y,
                        ),
                        size(1.0, caret_height),
                    ),
                    theme.accent(),
                );
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(_) => {
                self.cursor = self.text.chars().count();
                context.request_focus();
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::TextInput(text) => {
                self.insert(text, context);
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) => match key.key {
                Key::Named(NamedKey::Backspace) => {
                    self.backspace(context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Delete) => {
                    self.delete(context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.cursor = self.cursor.saturating_sub(1);
                    context.request_redraw();
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.cursor = (self.cursor + 1).min(self.text.chars().count());
                    context.request_redraw();
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Home) => {
                    self.cursor = 0;
                    context.request_redraw();
                    EventStatus::Handled
                }
                Key::Named(NamedKey::End) => {
                    self.cursor = self.text.chars().count();
                    context.request_redraw();
                    EventStatus::Handled
                }
                _ => EventStatus::Ignored,
            },
            InputEvent::FocusChanged(FocusChanged { focused }) => {
                self.focused = *focused == Some(context.current_target());
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

fn single_line(text: &str) -> String {
    text.chars()
        .filter(|character| !matches!(character, '\r' | '\n'))
        .collect()
}

fn byte_index(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

fn padding() -> Insets {
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{KeyCode, Modifiers, PointerButton, PointerButtons, PointerEvent, PointerId};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn commits_text_and_edits_at_unicode_scalar_boundaries() {
        let field = TextField::new();
        let changes = Rc::new(RefCell::new(Vec::new()));
        field.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |value| changes.borrow_mut().push(value.clone())
        });
        let mut runtime = UiRuntime::new(field);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("field layout succeeds");

        assert_eq!(
            runtime.dispatch_event(&pointer_down()),
            EventStatus::Handled
        );
        assert_eq!(
            runtime.dispatch_event(&InputEvent::TextInput("a🦀".to_owned())),
            EventStatus::Handled
        );
        assert_eq!(
            runtime.dispatch_event(&key(NamedKey::Backspace)),
            EventStatus::Handled
        );
        assert_eq!(
            runtime.dispatch_event(&InputEvent::TextInput("\nб".to_owned())),
            EventStatus::Handled
        );

        assert_eq!(*changes.borrow(), vec!["a🦀", "a", "aб"]);
    }

    #[test]
    fn navigation_moves_the_caret_before_insertions() {
        let field = TextField::with_text("ab");
        let changes = Rc::new(RefCell::new(Vec::new()));
        field.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |value| changes.borrow_mut().push(value.clone())
        });
        let mut runtime = UiRuntime::new(field);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("field layout succeeds");
        let _ = runtime.dispatch_event(&pointer_down());

        let _ = runtime.dispatch_event(&key(NamedKey::ArrowLeft));
        let _ = runtime.dispatch_event(&InputEvent::TextInput("X".to_owned()));

        assert_eq!(*changes.borrow(), vec!["aXb"]);
    }

    fn pointer_down() -> InputEvent {
        InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::ZERO,
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        })
    }

    fn key(key: NamedKey) -> InputEvent {
        InputEvent::KeyDown(torn_core::KeyEvent {
            key: Key::Named(key),
            code: KeyCode::Unidentified,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
    }
}
