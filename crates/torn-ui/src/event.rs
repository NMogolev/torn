use torn_core::{InputEvent, Point};

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

pub(crate) fn pointer_position(event: &InputEvent) -> Option<Point> {
    match event {
        InputEvent::PointerDown(event)
        | InputEvent::PointerMove(event)
        | InputEvent::PointerUp(event) => Some(event.position),
        InputEvent::Wheel(_)
        | InputEvent::KeyDown(_)
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
        InputEvent::Wheel(_)
        | InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => {}
    }
    event
}
