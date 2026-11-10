use torn_core::{
    Color, Constraints, InputEvent, Key, NamedKey, Point, PointerButton, PointerId, Rect, Size,
};
use torn_render::PaintContext;
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    StyleTarget, TssStyle, UiEnvironment, Widget,
};

const DEFAULT_WIDTH: f32 = 42.0;
const DEFAULT_HEIGHT: f32 = 24.0;
const DEFAULT_INSET: f32 = 3.0;

/// A compact two-state control with a sliding knob.
///
/// A switch is a more visual alternative to [`crate::Checkbox`] for settings
/// that take effect immediately. It owns no children and reports user changes
/// through [`Self::toggled`].
pub struct Switch {
    checked: bool,
    toggled: Signal<bool>,
    pressed_pointer: Option<PointerId>,
    size: Size,
    width: f32,
    height: f32,
    knob_inset: f32,
    on_color: Option<Color>,
    off_color: Option<Color>,
    knob_color: Option<Color>,
}

impl Switch {
    /// Creates an unchecked switch with the default track size.
    #[must_use]
    pub fn new() -> Self {
        Self::with_checked(false)
    }

    /// Creates a switch with the supplied initial state.
    #[must_use]
    pub fn with_checked(checked: bool) -> Self {
        Self {
            checked,
            toggled: Signal::new(),
            pressed_pointer: None,
            size: Size::ZERO,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            knob_inset: DEFAULT_INSET,
            on_color: None,
            off_color: None,
            knob_color: None,
        }
    }

    /// Returns whether the switch is on.
    #[must_use]
    pub const fn is_checked(&self) -> bool {
        self.checked
    }

    /// Sets the state without emitting [`Self::toggled`].
    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    /// Returns a signal emitted after user interaction changes the state.
    #[must_use]
    pub fn toggled(&self) -> Signal<bool> {
        self.toggled.clone()
    }

    /// Returns this switch with an explicit track size.
    #[must_use]
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = width.max(0.0);
        self.height = height.max(0.0);
        self
    }

    /// Returns this switch with `inset` logical pixels between the track edge
    /// and the knob.
    #[must_use]
    pub fn with_knob_inset(mut self, inset: f32) -> Self {
        self.knob_inset = inset.max(0.0);
        self
    }

    /// Returns this switch with an explicit checked track color.
    #[must_use]
    pub fn with_on_color(mut self, color: Color) -> Self {
        self.on_color = Some(color);
        self
    }

    /// Returns this switch with an explicit unchecked track color.
    #[must_use]
    pub fn with_off_color(mut self, color: Color) -> Self {
        self.off_color = Some(color);
        self
    }

    /// Returns this switch with an explicit knob color.
    #[must_use]
    pub fn with_knob_color(mut self, color: Color) -> Self {
        self.knob_color = Some(color);
        self
    }

    fn toggle(&mut self, context: &mut EventContext<'_>) {
        self.toggle_state();
        context.request_redraw();
    }

    fn toggle_state(&mut self) {
        self.checked = !self.checked;
        self.toggled.emit(&self.checked);
    }
}

impl Default for Switch {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Switch {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        self.size = constraints.constrain(size(self.width, self.height));
        LayoutResult::new(self.size)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("switch")
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
        let height = bounds.size.height().max(1.0);
        let radius = style
            .corner_radius
            .unwrap_or(height / 2.0)
            .min(height / 2.0);
        let checked_color = self
            .on_color
            .or(style.accent)
            .unwrap_or_else(|| theme.accent());
        let unchecked_color = self
            .off_color
            .or(style.background)
            .unwrap_or_else(|| theme.control_background());
        let track = if self.checked {
            checked_color
        } else {
            unchecked_color
        };
        context.fill_rounded_rect(bounds, radius, track);
        if let Some(border) = style.border {
            context.stroke_rounded_rect(bounds, radius, border.width, border.color);
        }

        let inset = self
            .knob_inset
            .min((bounds.size.height() - 2.0).max(0.0) / 2.0);
        let knob_diameter = (height - inset * 2.0).max(0.0);
        let travel = (bounds.size.width() - knob_diameter - inset * 2.0).max(0.0);
        let knob_x = bounds.origin.x + inset + if self.checked { travel } else { 0.0 };
        let knob = Rect::new(
            Point::new(knob_x, bounds.origin.y + inset),
            size(knob_diameter, knob_diameter),
        );
        let knob_color = self.knob_color.or(style.foreground).unwrap_or(Color::WHITE);
        context.fill_rounded_rect(knob, knob_diameter / 2.0, knob_color);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed_pointer = Some(pointer.pointer_id);
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pressed_pointer = None;
                context.release_pointer(pointer.pointer_id);
                if self.size.contains(pointer.position) {
                    self.toggle(context);
                }
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer)
                if self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pressed_pointer = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::KeyDown(key)
                if !key.repeat
                    && matches!(key.key, Key::Named(NamedKey::Space | NamedKey::Enter)) =>
            {
                self.toggle(context);
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::Switch)
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(
                AccessibilityActions::FOCUS
                    | AccessibilityActions::CLICK
                    | AccessibilityActions::TOGGLE,
            );
        if self.checked {
            accessibility = accessibility.with_states(AccessibilityState::CHECKED);
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        if matches!(
            action,
            AccessibilityAction::Click | AccessibilityAction::Toggle
        ) {
            self.toggle_state();
            true
        } else {
            false
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("switch sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{Constraints, KeyCode, Modifiers, PointerButtons, PointerEvent};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn toggles_with_pointer_and_keyboard() {
        let switch = Switch::new();
        let values = Rc::new(RefCell::new(Vec::new()));
        let _subscription = switch.toggled().subscribe({
            let values = Rc::clone(&values);
            move |value| values.borrow_mut().push(*value)
        });
        let mut runtime = UiRuntime::new(switch);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("switch layout succeeds");

        let _ = runtime.dispatch_event(&pointer(true));
        let _ = runtime.dispatch_event(&pointer(false));
        let _ = runtime.dispatch_event(&key());

        assert_eq!(*values.borrow(), vec![true, false]);
    }

    fn pointer(down: bool) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position: Point::ZERO,
            button: Some(PointerButton::Primary),
            buttons: if down {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        };
        if down {
            InputEvent::PointerDown(event)
        } else {
            InputEvent::PointerUp(event)
        }
    }

    fn key() -> InputEvent {
        InputEvent::KeyDown(torn_core::KeyEvent {
            key: Key::Named(NamedKey::Space),
            code: KeyCode::Unidentified,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
    }
}
