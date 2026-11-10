use torn_core::{
    Constraints, InputEvent, Key, NamedKey, Point, PointerButton, PointerId, Rect, Size,
};
use torn_render::PaintContext;
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    StyleTarget, TssStyle, UiEnvironment, Widget,
};

const DEFAULT_WIDTH: f32 = 160.0;
const CONTROL_HEIGHT: f32 = 24.0;
const TRACK_HEIGHT: f32 = 4.0;
const THUMB_RADIUS: f32 = 8.0;

/// A horizontal control for selecting a continuous value from a finite range.
pub struct Slider {
    minimum: f32,
    maximum: f32,
    value: f32,
    changed: Signal<f32>,
    dragging: Option<PointerId>,
    size: Size,
}

impl Slider {
    /// Creates a slider initialized to `minimum`.
    ///
    /// # Panics
    ///
    /// Panics when either bound is non-finite or `minimum` exceeds `maximum`.
    #[must_use]
    pub fn new(minimum: f32, maximum: f32) -> Self {
        Self::with_value(minimum, maximum, minimum)
    }

    /// Creates a slider with `value`, clamped to its finite inclusive range.
    ///
    /// # Panics
    ///
    /// Panics when a bound or value is non-finite, or when `minimum` exceeds
    /// `maximum`.
    #[must_use]
    pub fn with_value(minimum: f32, maximum: f32, value: f32) -> Self {
        assert_valid_range(minimum, maximum);
        assert!(value.is_finite(), "slider value must be finite");
        Self {
            minimum,
            maximum,
            value: value.clamp(minimum, maximum),
            changed: Signal::new(),
            dragging: None,
            size: Size::ZERO,
        }
    }

    /// Returns the minimum selectable value.
    #[must_use]
    pub const fn minimum(&self) -> f32 {
        self.minimum
    }

    /// Returns the maximum selectable value.
    #[must_use]
    pub const fn maximum(&self) -> f32 {
        self.maximum
    }

    /// Returns the current value.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.value
    }

    /// Sets the current value without emitting [`Self::changed`].
    ///
    /// # Panics
    ///
    /// Panics when `value` is non-finite.
    pub fn set_value(&mut self, value: f32) {
        assert!(value.is_finite(), "slider value must be finite");
        self.value = value.clamp(self.minimum, self.maximum);
    }

    /// Returns a signal emitted when the user changes the value.
    #[must_use]
    pub fn changed(&self) -> Signal<f32> {
        self.changed.clone()
    }

    fn set_user_value(&mut self, value: f32, context: &mut EventContext<'_>) {
        let value = value.clamp(self.minimum, self.maximum);
        if (self.value - value).abs() <= f32::EPSILON {
            return;
        }
        self.value = value;
        self.changed.emit(&self.value);
        context.request_redraw();
    }

    fn set_value_from_position(&mut self, position: Point, context: &mut EventContext<'_>) {
        let track = track_bounds(self.size);
        let ratio = if track.size.width() > 0.0 {
            ((position.x - track.origin.x) / track.size.width()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.set_user_value(
            self.minimum + (self.maximum - self.minimum) * ratio,
            context,
        );
    }

    fn keyboard_step(&self) -> f32 {
        (self.maximum - self.minimum) / 100.0
    }
}

impl Widget for Slider {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            DEFAULT_WIDTH
        };
        self.size = constraints.constrain(size(width, CONTROL_HEIGHT));
        LayoutResult::new(self.size)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("slider")
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
        let accent = style.accent.unwrap_or_else(|| theme.accent());
        let track = track_bounds(bounds.size);
        let track = offset_rect(track, bounds.origin);
        let thumb_center = track.origin.x + track.size.width() * self.value_ratio();
        let filled = Rect::new(
            track.origin,
            size(
                (thumb_center - track.origin.x).max(0.0),
                track.size.height(),
            ),
        );
        let thumb_radius = style
            .corner_radius
            .unwrap_or(THUMB_RADIUS)
            .min(THUMB_RADIUS);
        let thumb = Rect::new(
            Point::new(
                thumb_center - thumb_radius,
                bounds.origin.y + (bounds.size.height() - thumb_radius * 2.0) / 2.0,
            ),
            size(thumb_radius * 2.0, thumb_radius * 2.0),
        );
        let track_radius = style
            .corner_radius
            .unwrap_or(TRACK_HEIGHT / 2.0)
            .min(track.size.height() / 2.0);
        context.fill_rounded_rect(
            track,
            track_radius,
            style
                .background
                .unwrap_or_else(|| theme.control_background()),
        );
        if let Some(border) = style.border {
            context.stroke_rounded_rect(track, track_radius, border.width, border.color);
        }
        if filled.size.width() > 0.0 {
            context.fill_rounded_rect(filled, track_radius, accent);
        }
        context.fill_rounded_rect(thumb, thumb_radius, accent);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.dragging = Some(pointer.pointer_id);
                self.set_value_from_position(pointer.position, context);
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) if self.dragging == Some(pointer.pointer_id) => {
                self.set_value_from_position(pointer.position, context);
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) if self.dragging == Some(pointer.pointer_id) => {
                self.set_value_from_position(pointer.position, context);
                self.dragging = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer) if self.dragging == Some(pointer.pointer_id) => {
                self.dragging = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) if !key.repeat => match key.key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                    self.set_user_value(self.value - self.keyboard_step(), context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                    self.set_user_value(self.value + self.keyboard_step(), context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Home) => {
                    self.set_user_value(self.minimum, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::End) => {
                    self.set_user_value(self.maximum, context);
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
        Accessibility::new(AccessibilityRole::Slider)
            .with_numeric_value(
                f64::from(self.value),
                f64::from(self.minimum),
                f64::from(self.maximum),
            )
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(
                AccessibilityActions::FOCUS
                    | AccessibilityActions::SET_VALUE
                    | AccessibilityActions::INCREMENT
                    | AccessibilityActions::DECREMENT,
            )
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::SetValue(value) => {
                let Ok(value) = value.parse::<f32>() else {
                    return false;
                };
                self.set_accessibility_value(value);
                true
            }
            AccessibilityAction::Increment => {
                self.set_accessibility_value(self.value + self.keyboard_step());
                true
            }
            AccessibilityAction::Decrement => {
                self.set_accessibility_value(self.value - self.keyboard_step());
                true
            }
            _ => false,
        }
    }
}

impl Slider {
    fn set_accessibility_value(&mut self, value: f32) {
        if !value.is_finite() {
            return;
        }
        self.value = value.clamp(self.minimum, self.maximum);
        self.changed.emit(&self.value);
    }

    fn value_ratio(&self) -> f32 {
        let range = self.maximum - self.minimum;
        if range > 0.0 {
            (self.value - self.minimum) / range
        } else {
            0.0
        }
    }
}

fn assert_valid_range(minimum: f32, maximum: f32) {
    assert!(
        minimum.is_finite() && maximum.is_finite() && minimum <= maximum,
        "slider range must be finite and ordered"
    );
}

fn track_bounds(bounds_size: Size) -> Rect {
    let width = (bounds_size.width() - THUMB_RADIUS * 2.0).max(0.0);
    Rect::new(
        Point::new(
            THUMB_RADIUS.min(bounds_size.width() / 2.0),
            (bounds_size.height() - TRACK_HEIGHT) / 2.0,
        ),
        size(width, TRACK_HEIGHT.min(bounds_size.height())),
    )
}

fn offset_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
        rect.size,
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("slider sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{KeyCode, Modifiers, PointerButtons, PointerEvent};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn pointer_drag_and_keyboard_emit_clamped_values() {
        let slider = Slider::new(0.0, 100.0);
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = slider.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |value| changes.borrow_mut().push(*value)
        });
        let mut runtime = UiRuntime::new(slider);
        runtime
            .layout(Constraints::tight(size(100.0, CONTROL_HEIGHT)).expect("valid constraints"))
            .expect("slider layout succeeds");

        let _ = runtime.dispatch_event(&pointer(InputEventKind::Down, 8.0));
        let _ = runtime.dispatch_event(&pointer(InputEventKind::Move, 92.0));
        let _ = runtime.dispatch_event(&pointer(InputEventKind::Up, 120.0));
        let _ = runtime.dispatch_event(&key(NamedKey::ArrowLeft));

        assert_eq!(*changes.borrow(), vec![100.0, 99.0]);
    }

    #[test]
    fn programmatic_value_is_clamped_without_emitting() {
        let mut slider = Slider::with_value(10.0, 20.0, 30.0);
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = slider.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |value| changes.borrow_mut().push(*value)
        });

        slider.set_value(-5.0);

        assert!((slider.value() - 10.0).abs() <= f32::EPSILON);
        assert!(changes.borrow().is_empty());
    }

    #[derive(Clone, Copy)]
    enum InputEventKind {
        Down,
        Move,
        Up,
    }

    fn pointer(kind: InputEventKind, x: f32) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(x, 12.0),
            button: Some(PointerButton::Primary),
            buttons: match kind {
                InputEventKind::Down | InputEventKind::Move => PointerButtons::PRIMARY,
                InputEventKind::Up => PointerButtons::NONE,
            },
            modifiers: Modifiers::NONE,
        };
        match kind {
            InputEventKind::Down => InputEvent::PointerDown(event),
            InputEventKind::Move => InputEvent::PointerMove(event),
            InputEventKind::Up => InputEvent::PointerUp(event),
        }
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
