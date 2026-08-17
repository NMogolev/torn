use std::collections::HashMap;

use torn_core::{InputEvent, Point, PointerId, WidgetId};

/// The outcome of routing an input event to a widget.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EventStatus {
    /// The widget did not act on the event.
    #[default]
    Ignored,
    /// The widget handled the event.
    Handled,
}

impl EventStatus {
    /// Returns whether the event was handled.
    #[must_use]
    pub const fn is_handled(self) -> bool {
        matches!(self, Self::Handled)
    }
}

/// The point in propagation at which a widget receives an input event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPhase {
    /// The event travels from the root toward its target.
    Capture,
    /// The event is delivered to its hit-tested or focused target.
    Target,
    /// The event travels from the target's parent back to the root.
    Bubble,
}

/// Context supplied to a widget while it handles an input event.
///
/// It exposes routing side effects without requiring a global event system.
/// Requests take effect after the current handler returns, so a handler can
/// safely stop propagation and request pointer capture in the same call.
pub struct EventContext<'a> {
    phase: EventPhase,
    target: WidgetId,
    current_target: WidgetId,
    effects: &'a mut EventEffects,
}

impl<'a> EventContext<'a> {
    pub(crate) const fn new(
        phase: EventPhase,
        target: WidgetId,
        current_target: WidgetId,
        effects: &'a mut EventEffects,
    ) -> Self {
        Self {
            phase,
            target,
            current_target,
            effects,
        }
    }

    /// Returns the current propagation phase.
    #[must_use]
    pub const fn phase(&self) -> EventPhase {
        self.phase
    }

    /// Returns the widget that originally received the event.
    #[must_use]
    pub const fn target(&self) -> WidgetId {
        self.target
    }

    /// Returns the widget whose handler is currently running.
    #[must_use]
    pub const fn current_target(&self) -> WidgetId {
        self.current_target
    }

    /// Prevents delivery to subsequent widgets in the propagation route.
    pub fn stop_propagation(&mut self) {
        self.effects.propagation_stopped = true;
    }

    /// Routes subsequent events from `pointer_id` to the current widget.
    pub fn capture_pointer(&mut self, pointer_id: PointerId) {
        self.effects
            .pointer_capture
            .insert(pointer_id, self.current_target);
    }

    /// Releases the current widget's capture of `pointer_id`.
    pub fn release_pointer(&mut self, pointer_id: PointerId) {
        if self.effects.pointer_capture.get(&pointer_id) == Some(&self.current_target) {
            self.effects.pointer_capture.remove(&pointer_id);
        }
    }

    /// Requests keyboard focus for the current widget.
    pub fn request_focus(&mut self) {
        self.effects.focus_request = Some(FocusRequest::Set(self.current_target));
    }

    /// Removes keyboard focus when the current widget owns it.
    pub fn clear_focus(&mut self) {
        if self.effects.focused == Some(self.current_target) {
            self.effects.focus_request = Some(FocusRequest::Clear);
        }
    }

    /// Marks the UI as requiring another frame.
    pub fn request_redraw(&mut self) {
        self.effects.redraw_requested = true;
    }
}

pub(crate) struct EventEffects {
    pub(crate) propagation_stopped: bool,
    pub(crate) pointer_capture: HashMap<PointerId, WidgetId>,
    pub(crate) focused: Option<WidgetId>,
    pub(crate) focus_request: Option<FocusRequest>,
    pub(crate) redraw_requested: bool,
}

pub(crate) enum FocusRequest {
    Set(WidgetId),
    Clear,
}

pub(crate) fn pointer_position(event: &InputEvent) -> Option<Point> {
    match event {
        InputEvent::PointerDown(event)
        | InputEvent::PointerMove(event)
        | InputEvent::PointerUp(event) => Some(event.position),
        InputEvent::Wheel(event) => Some(event.position),
        InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => None,
    }
}

pub(crate) fn with_local_position(event: &InputEvent, origin: Point) -> InputEvent {
    let mut event = event.clone();
    match &mut event {
        InputEvent::PointerDown(pointer)
        | InputEvent::PointerMove(pointer)
        | InputEvent::PointerUp(pointer) => {
            pointer.position.x -= origin.x;
            pointer.position.y -= origin.y;
        }
        InputEvent::Wheel(wheel) => {
            wheel.position.x -= origin.x;
            wheel.position.y -= origin.y;
        }
        InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => {}
    }
    event
}
