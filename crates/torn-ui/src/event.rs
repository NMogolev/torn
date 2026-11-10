use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use torn_core::{Clipboard, InputEvent, Point, PointerId, WidgetId};

/// A window operation requested by a widget.
///
/// Widgets such as a custom title bar use this intent instead of depending on a
/// native windowing backend. The host application drains the requests from
/// [`crate::UiRuntime::take_window_commands`] and translates them into its
/// platform adapter's actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowCommand {
    /// Begin moving the native window while the primary pointer is held.
    StartDrag,
    /// Minimize the native window.
    Minimize,
    /// Toggle between maximized and restored window geometry.
    ToggleMaximize,
    /// Toggle borderless fullscreen mode.
    ToggleFullscreen,
    /// Ask the application to close the window.
    Close,
    /// Begin resizing the native window from `edge` while the primary pointer
    /// is held.
    StartResize(ResizeEdge),
}

/// An edge or corner of a window that can initiate a resize operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeEdge {
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
    /// Left edge.
    Left,
    /// Right edge.
    Right,
    /// Top-left corner.
    TopLeft,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-right corner.
    BottomRight,
}

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
    clipboard: &'a Rc<RefCell<Box<dyn Clipboard>>>,
}

impl<'a> EventContext<'a> {
    pub(crate) const fn new(
        phase: EventPhase,
        target: WidgetId,
        current_target: WidgetId,
        effects: &'a mut EventEffects,
        clipboard: &'a Rc<RefCell<Box<dyn Clipboard>>>,
    ) -> Self {
        Self {
            phase,
            target,
            current_target,
            effects,
            clipboard,
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

    /// Requests another layout pass before the next paint.
    ///
    /// Use this when event handling changes geometry, child visibility, or a
    /// child origin. A layout request also schedules a redraw.
    pub fn request_layout(&mut self) {
        self.effects.layout_requested = true;
        self.effects.redraw_requested = true;
    }

    /// Requests a native window operation from the host application.
    ///
    /// Requests are collected by the runtime and returned by
    /// [`crate::UiRuntime::take_window_commands`]. A widget does not need to
    /// know which windowing backend the application uses.
    pub fn request_window_command(&mut self, command: WindowCommand) {
        self.effects.window_commands.push(command);
    }

    /// Returns the clipboard's current plain text, if any.
    #[must_use]
    pub fn read_clipboard(&self) -> Option<String> {
        self.clipboard.borrow_mut().read_text()
    }

    /// Replaces the clipboard's plain-text contents.
    pub fn write_clipboard(&self, text: &str) {
        self.clipboard.borrow_mut().write_text(text);
    }
}

pub(crate) struct EventEffects {
    pub(crate) propagation_stopped: bool,
    pub(crate) pointer_capture: HashMap<PointerId, WidgetId>,
    pub(crate) focused: Option<WidgetId>,
    pub(crate) focus_request: Option<FocusRequest>,
    pub(crate) layout_requested: bool,
    pub(crate) redraw_requested: bool,
    pub(crate) window_commands: Vec<WindowCommand>,
}

pub(crate) enum FocusRequest {
    Set(WidgetId),
    Clear,
}

pub(crate) fn pointer_position(event: &InputEvent) -> Option<Point> {
    match event {
        InputEvent::PointerDown(event)
        | InputEvent::PointerMove(event)
        | InputEvent::PointerEnter(event)
        | InputEvent::PointerLeave(event)
        | InputEvent::PointerUp(event)
        | InputEvent::PointerCancel(event) => Some(event.position),
        InputEvent::Wheel(event) => Some(event.position),
        InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::TextPreedit(_)
        | InputEvent::FocusChanged(_)
        | InputEvent::WindowFocusChanged(_) => None,
    }
}

pub(crate) fn with_local_position(event: &InputEvent, origin: Point) -> InputEvent {
    let mut event = event.clone();
    match &mut event {
        InputEvent::PointerDown(pointer)
        | InputEvent::PointerMove(pointer)
        | InputEvent::PointerEnter(pointer)
        | InputEvent::PointerLeave(pointer)
        | InputEvent::PointerUp(pointer)
        | InputEvent::PointerCancel(pointer) => {
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
        | InputEvent::TextPreedit(_)
        | InputEvent::FocusChanged(_)
        | InputEvent::WindowFocusChanged(_) => {}
    }
    event
}
