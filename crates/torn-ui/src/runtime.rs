use std::{
    any::Any,
    collections::HashMap,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

use torn_core::{
    Constraints, Diagnostic, DiagnosticReporter, FocusChanged, InputEvent, Point, PointerId, Rect,
    WidgetId,
};
use torn_render::PaintContext;

use crate::{EventContext, EventPhase, EventStatus, LayoutResult, Widget, event};
use event::{EventEffects, FocusRequest};

/// Retained UI runtime responsible for layout, painting, input routing, and focus.
///
/// Pointer and wheel events use hit testing unless their pointer is captured.
/// Keyboard and text events are routed to the focused widget. A route travels
/// root-to-target during capture, reaches the target, then travels back during
/// bubble. Every recipient sees coordinates local to its own origin.
pub struct UiRuntime {
    root: Box<dyn Widget>,
    layout: Option<LayoutResult>,
    diagnostics: Vec<Diagnostic>,
    reporter: Option<Box<dyn DiagnosticReporter>>,
    pointer_capture: HashMap<PointerId, WidgetId>,
    focused: Option<WidgetId>,
    redraw_requested: bool,
}

/// Why a [`UiRuntime`] operation could not be completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiRuntimeError {
    /// Application widget code panicked during the requested operation.
    WidgetPanicked,
}

impl fmt::Display for UiRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WidgetPanicked => "application widget panicked; inspect runtime diagnostics",
        })
    }
}

impl std::error::Error for UiRuntimeError {}

#[derive(Clone, Debug)]
struct RouteNode {
    id: WidgetId,
    path: Vec<usize>,
    origin: Point,
    accepts_focus: bool,
}

impl UiRuntime {
    /// Creates a runtime that owns `root`.
    #[must_use]
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
            layout: None,
            diagnostics: Vec::new(),
            reporter: None,
            pointer_capture: HashMap::new(),
            focused: None,
            redraw_requested: true,
        }
    }

    /// Sets a reporter that receives every runtime diagnostic.
    pub fn set_diagnostic_reporter(&mut self, reporter: impl DiagnosticReporter + 'static) {
        self.reporter = Some(Box::new(reporter));
    }

    /// Removes the external diagnostic reporter, if one is configured.
    pub fn clear_diagnostic_reporter(&mut self) {
        self.reporter = None;
    }

    /// Returns diagnostics emitted since the runtime was created or last drained.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Drains and returns diagnostics emitted by the runtime.
    #[must_use]
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Lays out the root widget and returns its computed layout.
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::WidgetPanicked`] and emits a diagnostic if
    /// application widget code panics. The previous layout is discarded.
    pub fn layout(&mut self, constraints: Constraints) -> Result<&LayoutResult, UiRuntimeError> {
        match catch_unwind(AssertUnwindSafe(|| self.root.layout(constraints))) {
            Ok(layout) => {
                self.redraw_requested = true;
                Ok(self.layout.insert(layout))
            }
            Err(payload) => {
                self.layout = None;
                self.pointer_capture.clear();
                self.focused = None;
                self.report_panic("layout", payload);
                Err(UiRuntimeError::WidgetPanicked)
            }
        }
    }

    /// Returns the most recently computed root layout, if any.
    #[must_use]
    pub const fn last_layout(&self) -> Option<&LayoutResult> {
        self.layout.as_ref()
    }

    /// Returns the widget currently receiving keyboard input.
    #[must_use]
    pub const fn focused_widget(&self) -> Option<WidgetId> {
        self.focused
    }

    /// Returns the widget currently capturing `pointer_id`, if any.
    #[must_use]
    pub fn pointer_capture(&self, pointer_id: PointerId) -> Option<WidgetId> {
        self.pointer_capture.get(&pointer_id).copied()
    }

    /// Returns whether a handler requested another frame since the last call.
    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::replace(&mut self.redraw_requested, false)
    }

    /// Records the widget tree and reports a panic from application widget code.
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::WidgetPanicked`] if painting panics.
    pub fn paint(&mut self, context: &mut PaintContext<'_>) -> Result<(), UiRuntimeError> {
        if self.layout.is_none() {
            return Ok(());
        }

        match catch_unwind(AssertUnwindSafe(|| self.root.paint(context, Point::ZERO))) {
            Ok(()) => Ok(()),
            Err(payload) => {
                self.report_panic("paint", payload);
                Err(UiRuntimeError::WidgetPanicked)
            }
        }
    }

    /// Returns mutable access to the root widget and invalidates layout and input state.
    pub fn root_mut(&mut self) -> &mut dyn Widget {
        self.layout = None;
        self.pointer_capture.clear();
        self.focused = None;
        self.redraw_requested = true;
        &mut *self.root
    }

    /// Delivers an input event through capture, target, and bubble phases.
    ///
    /// Pointer and wheel events require a preceding layout. Keyboard and text
    /// events require a focused widget. If application handler code panics, the
    /// runtime records a diagnostic and stops the affected propagation route.
    pub fn dispatch_event(&mut self, event: &InputEvent) -> EventStatus {
        let routes = self.routes();
        let Some(route) = self.select_route(event, &routes) else {
            return EventStatus::Ignored;
        };

        let mut effects = EventEffects {
            propagation_stopped: false,
            pointer_capture: self.pointer_capture.clone(),
            focused: self.focused,
            focus_request: None,
            redraw_requested: false,
        };
        let mut handled = EventStatus::Ignored;

        let Some(target) = route.last() else {
            return EventStatus::Ignored;
        };
        let target_id = target.id;
        for node in route.iter().take(route.len().saturating_sub(1)) {
            handled = merge_status(
                handled,
                self.dispatch_to(node, target_id, EventPhase::Capture, event, &mut effects),
            );
            if effects.propagation_stopped {
                self.finish_event(event, effects, &routes);
                return handled;
            }
        }

        if let Some(target) = route.last() {
            handled = merge_status(
                handled,
                self.dispatch_to(target, target_id, EventPhase::Target, event, &mut effects),
            );
        }

        if !effects.propagation_stopped {
            for node in route.iter().rev().skip(1) {
                handled = merge_status(
                    handled,
                    self.dispatch_to(node, target_id, EventPhase::Bubble, event, &mut effects),
                );
                if effects.propagation_stopped {
                    break;
                }
            }
        }

        self.finish_event(event, effects, &routes);
        handled
    }

    fn finish_event(
        &mut self,
        event: &InputEvent,
        mut effects: EventEffects,
        routes: &[RouteNode],
    ) {
        if let Some(pointer_id) = pointer_id(event) {
            if matches!(event, InputEvent::PointerUp(_)) {
                effects.pointer_capture.remove(&pointer_id);
            }
        }
        self.pointer_capture = effects.pointer_capture;
        self.redraw_requested |= effects.redraw_requested;

        let Some(request) = effects.focus_request else {
            return;
        };
        let requested = match request {
            FocusRequest::Set(id) => routes
                .iter()
                .any(|node| node.id == id && node.accepts_focus)
                .then_some(id),
            FocusRequest::Clear => None,
        };
        if requested == self.focused {
            return;
        }
        self.focused = requested;
        self.redraw_requested = true;
        let focus_event = InputEvent::FocusChanged(FocusChanged { focused: requested });
        if let Some(target) = requested.and_then(|id| routes.iter().find(|node| node.id == id)) {
            let mut focus_effects = EventEffects {
                propagation_stopped: false,
                pointer_capture: self.pointer_capture.clone(),
                focused: self.focused,
                focus_request: None,
                redraw_requested: false,
            };
            let _ = self.dispatch_to(
                target,
                target.id,
                EventPhase::Target,
                &focus_event,
                &mut focus_effects,
            );
            self.pointer_capture = focus_effects.pointer_capture;
            self.redraw_requested |= focus_effects.redraw_requested;
        }
    }

    fn select_route(&self, event: &InputEvent, routes: &[RouteNode]) -> Option<Vec<RouteNode>> {
        let target = if let Some(pointer_id) = pointer_id(event) {
            self.pointer_capture
                .get(&pointer_id)
                .and_then(|captured| routes.iter().find(|node| node.id == *captured))
                .or_else(|| {
                    event::pointer_position(event)
                        .and_then(|position| self.hit_target(position, routes))
                })
        } else if matches!(event, InputEvent::Wheel(_)) {
            event::pointer_position(event).and_then(|position| self.hit_target(position, routes))
        } else {
            self.focused
                .and_then(|focused| routes.iter().find(|node| node.id == focused))
        }?;

        route_to(target, routes)
    }

    fn hit_target<'a>(&self, position: Point, routes: &'a [RouteNode]) -> Option<&'a RouteNode> {
        let layout = self.layout.as_ref()?;
        Rect::new(Point::ZERO, layout.size())
            .contains(position)
            .then_some(())?;
        let mut current = self.root.as_ref();
        let mut local_position = position;
        let mut path = Vec::new();
        while let Some((child_index, child_position)) = current.hit_test_child(local_position) {
            path.push(child_index);
            current = current.event_child_ref(child_index)?;
            local_position = child_position;
        }
        routes.iter().find(|node| node.path == path)
    }

    fn dispatch_to(
        &mut self,
        node: &RouteNode,
        target: WidgetId,
        phase: EventPhase,
        event: &InputEvent,
        effects: &mut EventEffects,
    ) -> EventStatus {
        let local_event = event::with_local_position(event, node.origin);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let widget = widget_at_path(&mut *self.root, &node.path)
                .expect("event route references a retained widget");
            let mut context = EventContext::new(phase, target, node.id, effects);
            widget.handle_event(&mut context, &local_event)
        }));
        match result {
            Ok(status) => status,
            Err(payload) => {
                effects.propagation_stopped = true;
                self.report_panic("event handler", payload);
                EventStatus::Ignored
            }
        }
    }

    fn routes(&self) -> Vec<RouteNode> {
        let mut routes = Vec::new();
        collect_routes(self.root.as_ref(), &mut routes, &[], Point::ZERO);
        routes
    }

    fn report_panic(&mut self, operation: &str, payload: Box<dyn Any + Send>) {
        let message = format!(
            "application widget panicked during {operation}: {}",
            panic_message(payload)
        );
        let diagnostic = Diagnostic::error("torn-ui", message);
        if let Some(reporter) = &mut self.reporter {
            reporter.report(&diagnostic);
        }
        self.diagnostics.push(diagnostic);
    }
}

fn collect_routes(widget: &dyn Widget, routes: &mut Vec<RouteNode>, path: &[usize], origin: Point) {
    let id = WidgetId::new(
        u32::try_from(routes.len()).expect("widget tree exceeds WidgetId capacity"),
        0,
    );
    routes.push(RouteNode {
        id,
        path: path.to_owned(),
        origin,
        accepts_focus: widget.accepts_focus(),
    });
    for index in 0..widget.event_child_count() {
        let Some(child) = widget.event_child_ref(index) else {
            continue;
        };
        let Some(child_origin) = widget.event_child_origin(index) else {
            continue;
        };
        let mut child_path = path.to_owned();
        child_path.push(index);
        collect_routes(
            child,
            routes,
            &child_path,
            Point::new(origin.x + child_origin.x, origin.y + child_origin.y),
        );
    }
}

fn route_to(target: &RouteNode, routes: &[RouteNode]) -> Option<Vec<RouteNode>> {
    let mut route = Vec::with_capacity(target.path.len() + 1);
    for depth in 0..=target.path.len() {
        let path = &target.path[..depth];
        route.push(routes.iter().find(|node| node.path == path)?.clone());
    }
    Some(route)
}

fn widget_at_path<'a>(widget: &'a mut dyn Widget, path: &[usize]) -> Option<&'a mut dyn Widget> {
    let Some((&head, tail)) = path.split_first() else {
        return Some(widget);
    };
    let child = widget.event_child(head)?;
    if tail.is_empty() {
        Some(child)
    } else {
        widget_at_path(child, tail)
    }
}

fn pointer_id(event: &InputEvent) -> Option<PointerId> {
    match event {
        InputEvent::PointerDown(event)
        | InputEvent::PointerMove(event)
        | InputEvent::PointerUp(event) => Some(event.pointer_id),
        InputEvent::Wheel(_)
        | InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => None,
    }
}

fn merge_status(left: EventStatus, right: EventStatus) -> EventStatus {
    if left.is_handled() || right.is_handled() {
        EventStatus::Handled
    } else {
        EventStatus::Ignored
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_owned(),
            Err(_) => "non-string panic payload".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{
        Constraints, InputEvent, Modifiers, Point, PointerButton, PointerButtons, PointerEvent,
        PointerId, Size,
    };

    use crate::{
        EventContext, EventPhase, EventStatus, LayoutResult, Row, UiRuntime, Widget, event,
    };

    type EventRecord = (&'static str, EventPhase, Point);
    type Events = Rc<RefCell<Vec<EventRecord>>>;

    struct Recorder {
        size: Size,
        name: &'static str,
        events: Events,
        capture_on_down: bool,
        focus_on_down: bool,
    }

    impl Widget for Recorder {
        fn layout(&mut self, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(self.size))
        }

        fn handle_event(
            &mut self,
            context: &mut EventContext<'_>,
            event: &InputEvent,
        ) -> EventStatus {
            if let Some(position) = event::pointer_position(event) {
                self.events
                    .borrow_mut()
                    .push((self.name, context.phase(), position));
            }
            if self.capture_on_down
                && matches!(event, InputEvent::PointerDown(_))
                && context.phase() == EventPhase::Target
            {
                if let InputEvent::PointerDown(pointer) = event {
                    context.capture_pointer(pointer.pointer_id);
                }
            }
            if self.focus_on_down
                && matches!(event, InputEvent::PointerDown(_))
                && context.phase() == EventPhase::Target
            {
                context.request_focus();
            }
            EventStatus::Handled
        }

        fn accepts_focus(&self) -> bool {
            self.focus_on_down
        }
    }

    struct Parent {
        child: Box<dyn Widget>,
        size: Size,
        name: &'static str,
        events: Events,
        stop_during_capture: bool,
    }

    impl Widget for Parent {
        fn layout(&mut self, constraints: Constraints) -> LayoutResult {
            let child_layout = self.child.layout(Constraints::loose(self.size));
            LayoutResult::with_children(
                constraints.constrain(self.size),
                vec![crate::ChildLayout::new(Point::ZERO, child_layout)],
            )
        }

        fn handle_event(
            &mut self,
            context: &mut EventContext<'_>,
            event: &InputEvent,
        ) -> EventStatus {
            if let Some(position) = event::pointer_position(event) {
                self.events
                    .borrow_mut()
                    .push((self.name, context.phase(), position));
            }
            if self.stop_during_capture && context.phase() == EventPhase::Capture {
                context.stop_propagation();
            }
            EventStatus::Handled
        }

        fn hit_test_child(&self, position: Point) -> Option<(usize, Point)> {
            (position.x >= 0.0
                && position.y >= 0.0
                && position.x < self.size.width()
                && position.y < self.size.height())
            .then_some((0, position))
        }

        fn event_child(&mut self, index: usize) -> Option<&mut (dyn Widget + '_)> {
            (index == 0).then_some(self.child.as_mut())
        }

        fn event_child_ref(&self, index: usize) -> Option<&(dyn Widget + '_)> {
            (index == 0).then_some(self.child.as_ref())
        }

        fn event_child_count(&self) -> usize {
            1
        }

        fn event_child_origin(&self, index: usize) -> Option<Point> {
            (index == 0).then_some(Point::ZERO)
        }
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    fn pointer(position: Point, down: bool) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position,
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

    #[test]
    fn routes_events_through_capture_target_and_bubble_with_local_coordinates() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let target = Recorder {
            size: size(20.0, 10.0),
            name: "target",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: false,
        };
        let parent = Parent {
            child: Box::new(target),
            size: size(20.0, 10.0),
            name: "parent",
            events: Rc::clone(&events),
            stop_during_capture: false,
        };
        let root = Parent {
            child: Box::new(parent),
            size: size(20.0, 10.0),
            name: "root",
            events: Rc::clone(&events),
            stop_during_capture: false,
        };
        let mut runtime = UiRuntime::new(root);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");

        assert_eq!(
            runtime.dispatch_event(&pointer(Point::new(15.0, 5.0), true)),
            EventStatus::Handled
        );
        assert_eq!(
            *events.borrow(),
            vec![
                ("root", EventPhase::Capture, Point::new(15.0, 5.0)),
                ("parent", EventPhase::Capture, Point::new(15.0, 5.0)),
                ("target", EventPhase::Target, Point::new(15.0, 5.0)),
                ("parent", EventPhase::Bubble, Point::new(15.0, 5.0)),
                ("root", EventPhase::Bubble, Point::new(15.0, 5.0)),
            ]
        );
    }

    #[test]
    fn captured_pointer_routes_outside_original_bounds() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut row = Row::new();
        row.push(Recorder {
            size: size(10.0, 10.0),
            name: "capturing",
            events: Rc::clone(&events),
            capture_on_down: true,
            focus_on_down: false,
        });
        row.push(Recorder {
            size: size(10.0, 10.0),
            name: "other",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: false,
        });
        let mut runtime = UiRuntime::new(row);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true));
        let _ = runtime.dispatch_event(&pointer(Point::new(15.0, 5.0), false));
        assert!(runtime.pointer_capture(PointerId(1)).is_none());
        assert_eq!(
            events.borrow().last().map(|event| event.0),
            Some("capturing")
        );
    }

    #[test]
    fn routes_keyboard_events_to_the_widget_that_requested_focus() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(Recorder {
            size: size(10.0, 10.0),
            name: "focused",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: true,
        });
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true));
        let focused = runtime.focused_widget().expect("widget requested focus");
        assert_eq!(
            runtime.dispatch_event(&InputEvent::TextInput("тест".to_owned())),
            EventStatus::Handled
        );

        assert_eq!(runtime.focused_widget(), Some(focused));
        assert_eq!(events.borrow().len(), 1);
    }

    #[test]
    fn stopping_capture_prevents_target_and_bubble_delivery() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let target = Recorder {
            size: size(10.0, 10.0),
            name: "target",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: false,
        };
        let root = Parent {
            child: Box::new(target),
            size: size(10.0, 10.0),
            name: "root",
            events: Rc::clone(&events),
            stop_during_capture: true,
        };
        let mut runtime = UiRuntime::new(root);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");

        assert_eq!(
            runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true)),
            EventStatus::Handled
        );
        assert_eq!(
            *events.borrow(),
            vec![("root", EventPhase::Capture, Point::new(5.0, 5.0))]
        );
    }
}
