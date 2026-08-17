use torn_core::{Constraints, InputEvent, Point, Rect};
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
}

impl UiRuntime {
    /// Creates a runtime that owns `root`.
    #[must_use]
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
            layout: None,
        }
    }

    /// Lays out the root widget and returns its computed layout.
    pub fn layout(&mut self, constraints: Constraints) -> &LayoutResult {
        self.layout.insert(self.root.layout(constraints))
    }

    /// Returns the most recently computed root layout, if any.
    #[must_use]
    pub const fn last_layout(&self) -> Option<&LayoutResult> {
        self.layout.as_ref()
    }

    /// Records the laid-out widget tree into `context`.
    ///
    /// Does nothing until [`Self::layout`] has been called.
    pub fn paint(&self, context: &mut PaintContext<'_>) {
        if self.layout.is_some() {
            self.root.paint(context, Point::ZERO);
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

        self.root
            .handle_event(&event::with_local_position(event, Point::ZERO))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{
        Constraints, InputEvent, Modifiers, Point, PointerButton, PointerButtons, PointerEvent,
        PointerId, Size,
    };

    use crate::{Column, EventStatus, LayoutResult, Row, UiRuntime, Widget};

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
        runtime.layout(Constraints::UNBOUNDED);

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
        runtime.layout(Constraints::UNBOUNDED);
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
        runtime.layout(Constraints::UNBOUNDED);

        assert_eq!(
            runtime.dispatch_event(&click(Point::new(15.0, 5.0))),
            EventStatus::Handled
        );
        assert_eq!(*clicks.borrow(), vec![Point::new(5.0, 5.0)]);
    }
}
