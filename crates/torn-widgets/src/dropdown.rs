use torn_core::{Constraints, InputEvent, Insets, Key, NamedKey, Point, PointerButton, Rect, Size};
use torn_render::{PaintContext, TextStyle};
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    StyleTarget, TssStyle, UiEnvironment, Widget,
};

const DEFAULT_WIDTH: f32 = 120.0;
const HORIZONTAL_PADDING: f32 = 8.0;
const VERTICAL_PADDING: f32 = 6.0;
const ARROW_WIDTH: f32 = 16.0;

/// A compact selection control with an inline, expandable list of text options.
///
/// The list is part of the widget's normal layout when open. This keeps its
/// options compatible with the retained tree's clipping, hit testing, and focus
/// model; a floating overlay can be introduced later without changing the
/// selection API.
pub struct Dropdown {
    options: Vec<String>,
    selected: Option<usize>,
    changed: Signal<usize>,
    open: bool,
    size: Size,
    header_height: f32,
    row_height: f32,
}

impl Dropdown {
    /// Creates a dropdown from text options without an initial selection.
    #[must_use]
    pub fn new<T>(options: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<String>,
    {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: None,
            changed: Signal::new(),
            open: false,
            size: Size::ZERO,
            header_height: 0.0,
            row_height: 0.0,
        }
    }

    /// Returns the available options in display order.
    #[must_use]
    pub fn options(&self) -> &[String] {
        &self.options
    }

    /// Returns the selected option index, if one is selected.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Returns the selected option text, if one is selected.
    #[must_use]
    pub fn selected_option(&self) -> Option<&str> {
        self.selected
            .and_then(|index| self.options.get(index).map(String::as_str))
    }

    /// Sets the selected option without emitting [`Self::changed`].
    ///
    /// # Panics
    ///
    /// Panics if `selected` does not refer to an available option.
    pub fn set_selected(&mut self, selected: Option<usize>) {
        self.assert_valid_selection(selected);
        self.selected = selected;
    }

    /// Returns whether the options list is currently expanded.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Returns a signal emitted when a user selects a different option.
    #[must_use]
    pub fn changed(&self) -> Signal<usize> {
        self.changed.clone()
    }

    fn set_open(&mut self, open: bool, context: &mut EventContext<'_>) {
        if self.open == open {
            return;
        }
        self.open = open;
        context.request_layout();
    }

    fn select_user(&mut self, selected: usize, context: &mut EventContext<'_>) {
        self.assert_valid_selection(Some(selected));
        if self.selected == Some(selected) {
            self.set_open(false, context);
            return;
        }
        self.selected = Some(selected);
        self.changed.emit(&selected);
        self.set_open(false, context);
        context.request_redraw();
    }

    fn select_relative(&mut self, offset: isize, context: &mut EventContext<'_>) {
        let Some(last) = self.options.len().checked_sub(1) else {
            return;
        };
        let selected = self
            .selected
            .unwrap_or(if offset.is_negative() { last } else { 0 });
        let next = selected.saturating_add_signed(offset).min(last);
        self.select_user(next, context);
    }

    fn option_at(&self, point: Point) -> Option<usize> {
        if !self.open || self.row_height <= 0.0 || point.y < self.header_height {
            return None;
        }
        self.options.iter().enumerate().find_map(|(index, _)| {
            let top = self.header_height + option_offset(index, self.row_height);
            (point.y >= top && point.y < top + self.row_height).then_some(index)
        })
    }

    fn assert_valid_selection(&self, selected: Option<usize>) {
        assert!(
            selected.is_none_or(|index| index < self.options.len()),
            "dropdown selection must refer to an available option"
        );
    }

    fn select_without_context(&mut self, selected: usize) {
        self.assert_valid_selection(Some(selected));
        if self.selected != Some(selected) {
            self.selected = Some(selected);
            self.changed.emit(&selected);
        }
        self.open = false;
    }

    fn accept_accessibility_value(&mut self, value: &str) -> bool {
        let Some(index) = self.options.iter().position(|option| option == value) else {
            return false;
        };
        self.select_without_context(index);
        true
    }

    fn accept_accessibility_step(&mut self, offset: isize) -> bool {
        let Some(last) = self.options.len().checked_sub(1) else {
            return false;
        };
        let selected = self
            .selected
            .unwrap_or(if offset.is_negative() { last } else { 0 });
        let next = selected.saturating_add_signed(offset).min(last);
        self.select_without_context(next);
        true
    }
}

impl Widget for Dropdown {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let font = environment.font_for(&style);
        let font_size = font.pixel_size(environment.logical_dpi());
        let foreground = style.foreground_or(environment.theme().foreground());
        let text_style = TextStyle::new_with_font(font, foreground);
        let padding = style.padding_or(default_padding());
        self.header_height = (font_size + padding.vertical()).max(24.0);
        self.row_height = self.header_height;
        let intrinsic_width = self.options.iter().fold(DEFAULT_WIDTH, |width, option| {
            let layout = environment.text_shaper().layout(option, &text_style, None);
            width.max(layout.size().width() + padding.horizontal() + ARROW_WIDTH)
        });
        let width = if constraints.max().width().is_finite() {
            constraints.max().width()
        } else {
            intrinsic_width
        };
        let height = self.header_height
            + if self.open {
                option_offset(self.options.len(), self.row_height)
            } else {
                0.0
            };
        self.size = constraints.constrain(size(width, height));
        LayoutResult::new(self.size)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("dropdown")
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
        let font = environment.font_for(style);
        let text_style = TextStyle::new_with_font(font, foreground);
        let padding = style.padding_or(default_padding());
        let header = Rect::new(bounds.origin, size(bounds.size.width(), self.header_height));
        let transition = context.transition();
        let accent = style.accent_or(theme.accent());
        context.fill_rounded_rect(
            header,
            radius,
            style.background.unwrap_or_else(|| {
                transition.control_color(
                    theme.control_background(),
                    theme.control_hover_background(),
                    theme.control_pressed_background(),
                )
            }),
        );
        let border = style.border.unwrap_or_else(|| {
            torn_ui::Border::new(1.0, transition.focus_color(theme.border(), accent))
        });
        context.stroke_rounded_rect(header, radius, border.width, border.color);
        if let Some(option) = self.selected_option() {
            let text = environment.text_shaper().layout(
                option,
                &text_style,
                Some((header.size.width() - padding.horizontal() - ARROW_WIDTH).max(0.0)),
            );
            context.draw_text(
                text,
                Point::new(
                    header.origin.x + padding.left,
                    header.origin.y + padding.top,
                ),
            );
        }
        let arrow = Rect::new(
            Point::new(
                header.right() - padding.right - 6.0,
                header.origin.y + header.size.height() / 2.0 - 2.0,
            ),
            size(6.0, 4.0),
        );
        context.fill_rect(arrow, foreground);

        if !self.open {
            return;
        }
        context.with_clip(bounds, |context| {
            let popup = Rect::new(
                Point::new(bounds.origin.x, bounds.origin.y + self.header_height),
                size(
                    bounds.size.width(),
                    (bounds.size.height() - self.header_height).max(0.0),
                ),
            );
            context.fill_rect(popup, theme.raised_surface());
            for (index, option) in self.options.iter().enumerate() {
                let row = Rect::new(
                    Point::new(
                        bounds.origin.x,
                        bounds.origin.y
                            + self.header_height
                            + option_offset(index, self.row_height),
                    ),
                    size(bounds.size.width(), self.row_height),
                );
                if self.selected == Some(index) {
                    context.fill_rect(row, style.background.unwrap_or_else(|| theme.selection()));
                    context.fill_rect(Rect::new(row.origin, size(3.0, row.size.height())), accent);
                }
                let text = environment.text_shaper().layout(
                    option,
                    &text_style,
                    Some((row.size.width() - padding.horizontal()).max(0.0)),
                );
                context.draw_text(
                    text,
                    Point::new(row.origin.x + padding.left, row.origin.y + padding.top),
                );
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                context.request_focus();
                if let Some(index) = self.option_at(pointer.position) {
                    self.select_user(index, context);
                } else if pointer.position.y < self.header_height {
                    self.set_open(!self.open, context);
                }
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) if !key.repeat => match key.key {
                Key::Named(NamedKey::Enter | NamedKey::Space) => {
                    self.set_open(!self.open, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Escape) if self.open => {
                    self.set_open(false, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.select_relative(1, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.select_relative(-1, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Home) if !self.options.is_empty() => {
                    self.select_user(0, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::End) if !self.options.is_empty() => {
                    self.select_user(self.options.len() - 1, context);
                    EventStatus::Handled
                }
                _ => EventStatus::Ignored,
            },
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::ComboBox)
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(
                AccessibilityActions::FOCUS
                    | AccessibilityActions::CLICK
                    | AccessibilityActions::SET_VALUE
                    | AccessibilityActions::EXPAND
                    | AccessibilityActions::COLLAPSE
                    | AccessibilityActions::INCREMENT
                    | AccessibilityActions::DECREMENT,
            );
        if let Some(option) = self.selected_option() {
            accessibility = accessibility.with_value(option);
        }
        if self.open {
            accessibility = accessibility.with_states(AccessibilityState::EXPANDED);
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::Click => {
                self.open = !self.open;
                true
            }
            AccessibilityAction::Expand => {
                self.open = true;
                true
            }
            AccessibilityAction::Collapse => {
                self.open = false;
                true
            }
            AccessibilityAction::SetValue(value) => self.accept_accessibility_value(value),
            AccessibilityAction::Increment => self.accept_accessibility_step(1),
            AccessibilityAction::Decrement => self.accept_accessibility_step(-1),
            _ => false,
        }
    }
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
    Size::new(width, height).expect("dropdown sizes are non-negative and not NaN")
}

#[allow(clippy::cast_precision_loss)]
fn option_offset(index: usize, row_height: f32) -> f32 {
    // UI coordinates are f32 throughout. At magnitudes where an index loses
    // precision, individual logical pixels cannot be distinguished either.
    index as f32 * row_height
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{KeyCode, Modifiers, PointerButtons, PointerEvent, PointerId};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn opens_and_selects_an_option_with_the_pointer() {
        let dropdown = Dropdown::new(["One", "Two"]);
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = dropdown.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |index| changes.borrow_mut().push(*index)
        });
        let mut runtime = UiRuntime::new(dropdown);
        let constraints = Constraints::tight(size(100.0, 100.0)).expect("valid constraints");
        runtime
            .layout(constraints)
            .expect("dropdown layout succeeds");

        let _ = runtime.dispatch_event(&pointer(1.0));
        runtime
            .layout(constraints)
            .expect("expanded layout succeeds");
        let _ = runtime.dispatch_event(&pointer(30.0));

        assert_eq!(*changes.borrow(), vec![0]);
    }

    #[test]
    fn keyboard_selects_and_closes_the_list() {
        let mut runtime = UiRuntime::new(Dropdown::new(["One", "Two"]));
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("dropdown layout succeeds");
        let _ = runtime.dispatch_event(&pointer(1.0));
        let _ = runtime.dispatch_event(&key(NamedKey::ArrowDown));

        assert!(runtime.last_layout().is_none());
    }

    fn pointer(y: f32) -> InputEvent {
        InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(1.0, y),
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
