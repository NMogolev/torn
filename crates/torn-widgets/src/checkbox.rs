use torn_core::{Color, Constraints, InputEvent, Key, NamedKey, Point, PointerButton, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, Border, ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext,
    LayoutResult, Signal, StyleTarget, TssStyle, UiEnvironment, Widget,
};

const CONTROL_SIZE: f32 = 18.0;
const MARK_INSET: f32 = 5.0;

/// A two-state control with an optional runtime-owned label child.
pub struct Checkbox {
    checked: bool,
    toggled: Signal<bool>,
    pressed_pointer: Option<torn_core::PointerId>,
    size: Size,
}

impl Checkbox {
    /// Creates an unchecked checkbox.
    #[must_use]
    pub fn new() -> Self {
        Self::with_checked(false)
    }

    /// Creates a checkbox with the supplied initial state.
    #[must_use]
    pub fn with_checked(checked: bool) -> Self {
        Self {
            checked,
            toggled: Signal::new(),
            pressed_pointer: None,
            size: Size::ZERO,
        }
    }

    /// Returns whether this checkbox is checked.
    #[must_use]
    pub const fn is_checked(&self) -> bool {
        self.checked
    }

    /// Sets the checked state without emitting [`Self::toggled`].
    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    /// Returns a signal emitted after user interaction changes the checked state.
    #[must_use]
    pub fn toggled(&self) -> Signal<bool> {
        self.toggled.clone()
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

impl Default for Checkbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Checkbox {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let gap = context
            .style()
            .spacing
            .unwrap_or_else(|| context.environment().theme().spacing());
        let mut children = Vec::new();
        let label_size = if context.child_count() == 1 {
            let max = constraints.max();
            let child_max = size((max.width() - CONTROL_SIZE - gap).max(0.0), max.height());
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(
                id,
                Point::new(
                    CONTROL_SIZE + gap,
                    (CONTROL_SIZE.max(layout.size().height()) - layout.size().height()) / 2.0,
                ),
            ));
            layout.size()
        } else {
            Size::ZERO
        };
        let size = constraints.constrain(size(
            CONTROL_SIZE
                + if context.child_count() == 1 {
                    gap + label_size.width()
                } else {
                    0.0
                },
            CONTROL_SIZE.max(label_size.height()),
        ));
        self.size = size;
        LayoutResult::with_children(size, children)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("checkbox")
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
        let indicator = indicator_bounds(bounds);
        let theme = environment.theme();
        let accent = style.accent.unwrap_or_else(|| theme.accent());
        let background = style.background.unwrap_or(if self.checked {
            accent
        } else {
            theme.background()
        });
        let border = style.border.unwrap_or_else(|| {
            Border::new(
                1.0,
                if self.checked {
                    accent
                } else {
                    theme.control_background()
                },
            )
        });
        let radius = style
            .corner_radius
            .unwrap_or_else(|| theme.corner_radius())
            .min(CONTROL_SIZE / 2.0);
        context.fill_rounded_rect(indicator, radius, background);
        context.stroke_rounded_rect(indicator, radius, border.width, border.color);
        if self.checked {
            context.fill_rect(
                indicator.inset(torn_core::Insets::all(MARK_INSET)),
                style.foreground.unwrap_or(Color::WHITE),
            );
        }
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
        let mut accessibility = Accessibility::new(AccessibilityRole::CheckBox)
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

fn indicator_bounds(bounds: Rect) -> Rect {
    Rect::new(
        Point::new(
            bounds.origin.x,
            bounds.origin.y + (bounds.size.height() - CONTROL_SIZE).max(0.0) / 2.0,
        ),
        size(CONTROL_SIZE, CONTROL_SIZE),
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("checkbox sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{KeyCode, Modifiers, PointerButtons, PointerEvent, PointerId};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn toggles_with_pointer_and_keyboard() {
        let checkbox = Checkbox::new();
        let values = Rc::new(RefCell::new(Vec::new()));
        let _subscription = checkbox.toggled().subscribe({
            let values = Rc::clone(&values);
            move |value| values.borrow_mut().push(*value)
        });
        let mut runtime = UiRuntime::new(checkbox);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("checkbox layout succeeds");

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
