use std::{
    any::Any,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

use torn_core::{Constraints, Diagnostic, DiagnosticReporter, InputEvent, Point, Rect};
use torn_render::PaintContext;

use crate::{EventStatus, LayoutResult, Widget, event};

/// A minimal retained UI runtime that lays out a root widget and routes pointer events.
///
/// This initial runtime performs direct hit testing only: an event is delivered to
/// the topmost hit target and does not bubble to its ancestors. Pointer positions
/// passed to widgets are relative to the target widget's origin. Capture, focus,
/// and keyboard routing are introduced in a later event-system milestone.
pub struct UiRuntime {
    root: Box<dyn Widget>,
    layout: Option<LayoutResult>,
    diagnostics: Vec<Diagnostic>,
    reporter: Option<Box<dyn DiagnosticReporter>>,
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

impl UiRuntime {
    /// Creates a runtime that owns `root`.
    #[must_use]
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
            layout: None,
            diagnostics: Vec::new(),
            reporter: None,
        }
    }

    /// Sets a reporter that receives every runtime diagnostic.
    ///
    /// Diagnostics continue to be available through [`Self::diagnostics`] and
    /// [`Self::take_diagnostics`] after they are forwarded to `reporter`.
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
            Ok(layout) => Ok(self.layout.insert(layout)),
            Err(payload) => {
                self.layout = None;
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

    /// Returns mutable access to the root widget and invalidates its layout.
    pub fn root_mut(&mut self) -> &mut dyn Widget {
        self.layout = None;
        &mut *self.root
    }

    /// Routes a pointer event to the widget at its position.
    ///
    /// Events are ignored until [`Self::layout`] has been called. Non-pointer
    /// events are ignored by this initial direct-hit-test runtime.
    pub fn dispatch_event(&mut self, event: &InputEvent) -> EventStatus {
        let Some(position) = event::pointer_position(event) else {
            return EventStatus::Ignored;
        };
        let Some(layout) = &self.layout else {
            return EventStatus::Ignored;
        };
        let bounds = Rect::new(Point::ZERO, layout.size());
        if !bounds.contains(position) {
            return EventStatus::Ignored;
        }

        match catch_unwind(AssertUnwindSafe(|| {
            self.root
                .handle_event(&event::with_local_position(event, Point::ZERO))
        })) {
            Ok(status) => status,
            Err(payload) => {
                self.report_panic("event handler", payload);
                EventStatus::Ignored
            }
        }
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
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use torn_core::{
        Constraints, Diagnostic, InputEvent, Modifiers, Point, PointerButton, PointerButtons,
        PointerEvent, PointerId, Size,
    };

    use crate::{Column, EventStatus, LayoutResult, Row, UiRuntime, UiRuntimeError, Widget};

    struct ClickRecorder {
        size: Size,
        clicks: Rc<RefCell<Vec<Point>>>,
    }

    impl Widget for ClickRecorder {
        fn layout(&mut self, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(self.size))
        }

        fn handle_event(&mut self, event: &InputEvent) -> EventStatus {
            if let InputEvent::PointerDown(event) = event {
                self.clicks.borrow_mut().push(event.position);
                EventStatus::Handled
            } else {
                EventStatus::Ignored
            }
        }
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    fn click(position: Point) -> InputEvent {
        InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        })
    }

    #[test]
    fn routes_a_click_to_its_hit_target_with_local_coordinates() {
        let left_clicks = Rc::new(RefCell::new(Vec::new()));
        let right_clicks = Rc::new(RefCell::new(Vec::new()));
        let mut row = Row::new();
        row.push(ClickRecorder {
            size: size(20.0, 10.0),
            clicks: Rc::clone(&left_clicks),
        });
        row.push(ClickRecorder {
            size: size(30.0, 10.0),
            clicks: Rc::clone(&right_clicks),
        });

        let mut runtime = UiRuntime::new(row);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("widget does not panic");

        assert_eq!(
            runtime.dispatch_event(&click(Point::new(25.0, 5.0))),
            EventStatus::Handled
        );
        assert!(left_clicks.borrow().is_empty());
        assert_eq!(*right_clicks.borrow(), vec![Point::new(5.0, 5.0)]);
    }

    #[test]
    fn ignores_clicks_outside_the_root_or_before_layout() {
        let clicks = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(ClickRecorder {
            size: size(20.0, 10.0),
            clicks: Rc::clone(&clicks),
        });

        assert_eq!(
            runtime.dispatch_event(&click(Point::new(5.0, 5.0))),
            EventStatus::Ignored
        );
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("widget does not panic");
        assert_eq!(
            runtime.dispatch_event(&click(Point::new(20.0, 5.0))),
            EventStatus::Ignored
        );
        assert!(clicks.borrow().is_empty());
    }

    #[test]
    fn routes_through_nested_containers() {
        let clicks = Rc::new(RefCell::new(Vec::new()));
        let mut row = Row::new();
        row.push(ClickRecorder {
            size: size(10.0, 10.0),
            clicks: Rc::new(RefCell::new(Vec::new())),
        });
        row.push(ClickRecorder {
            size: size(20.0, 10.0),
            clicks: Rc::clone(&clicks),
        });

        let mut column = Column::new();
        column.push(row);
        let mut runtime = UiRuntime::new(column);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("widget does not panic");

        assert_eq!(
            runtime.dispatch_event(&click(Point::new(15.0, 5.0))),
            EventStatus::Handled
        );
        assert_eq!(*clicks.borrow(), vec![Point::new(5.0, 5.0)]);
    }

    struct PanickingWidget;

    impl Widget for PanickingWidget {
        fn layout(&mut self, _constraints: Constraints) -> LayoutResult {
            panic!("invalid application layout")
        }
    }

    #[test]
    fn turns_a_widget_panic_into_a_collectable_diagnostic() {
        let mut runtime = UiRuntime::new(PanickingWidget);

        assert_eq!(
            runtime.layout(Constraints::UNBOUNDED),
            Err(UiRuntimeError::WidgetPanicked)
        );
        assert!(runtime.last_layout().is_none());
        assert_eq!(runtime.diagnostics().len(), 1);
        assert_eq!(runtime.diagnostics()[0].component(), "torn-ui");
        assert!(
            runtime.diagnostics()[0]
                .message()
                .contains("invalid application layout")
        );
        assert_eq!(runtime.take_diagnostics().len(), 1);
        assert!(runtime.diagnostics().is_empty());
    }

    #[test]
    fn forwards_diagnostics_to_an_external_reporter() {
        let reports = Rc::new(Cell::new(0));
        let mut runtime = UiRuntime::new(PanickingWidget);
        runtime.set_diagnostic_reporter({
            let reports = Rc::clone(&reports);
            move |_: &Diagnostic| reports.set(reports.get() + 1)
        });

        assert_eq!(
            runtime.layout(Constraints::UNBOUNDED),
            Err(UiRuntimeError::WidgetPanicked)
        );
        assert_eq!(reports.get(), 1);
    }
}
