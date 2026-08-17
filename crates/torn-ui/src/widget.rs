use torn_core::{Constraints, InputEvent, Point};
use torn_render::PaintContext;

use crate::{EventContext, EventStatus, LayoutResult};

/// A retained UI element that can determine its size and place its children.
///
/// Implementations must return a finite size within `constraints`. Child bounds
/// in the returned [`LayoutResult`] are relative to this widget's origin.
pub trait Widget {
    /// Lays out this widget and its descendants within `constraints`.
    fn layout(&mut self, constraints: Constraints) -> LayoutResult;

    /// Records this widget's paint operations at `origin`.
    ///
    /// The origin is relative to the display list's coordinate system. The
    /// default implementation records no paint operations.
    fn paint(&self, _context: &mut PaintContext<'_>, _origin: Point) {}

    /// Handles an input event during capture, target, or bubble propagation.
    ///
    /// Pointer and wheel positions are relative to this widget's origin. The
    /// default implementation ignores the event, so widgets interested only in
    /// layout do not need an event handler.
    fn handle_event(
        &mut self,
        _context: &mut EventContext<'_>,
        _event: &InputEvent,
    ) -> EventStatus {
        EventStatus::Ignored
    }

    /// Returns whether this widget may receive keyboard focus.
    fn accepts_focus(&self) -> bool {
        false
    }

    /// Returns the direct child hit by `position` in this widget's coordinates.
    ///
    /// The returned point is relative to that child's origin. Container widgets
    /// override this together with [`Self::event_child`] to expose their retained
    /// structure to the runtime's event router.
    fn hit_test_child(&self, _position: Point) -> Option<(usize, Point)> {
        None
    }

    /// Returns one direct child for event routing.
    fn event_child(&mut self, _index: usize) -> Option<&mut (dyn Widget + '_)> {
        None
    }

    /// Returns one direct child for immutable event-tree traversal.
    fn event_child_ref(&self, _index: usize) -> Option<&(dyn Widget + '_)> {
        None
    }

    /// Returns the direct children currently retained by this widget.
    fn event_child_count(&self) -> usize {
        0
    }

    /// Returns the origin of a direct child relative to this widget.
    fn event_child_origin(&self, _index: usize) -> Option<Point> {
        None
    }
}

impl<W> Widget for Box<W>
where
    W: Widget + ?Sized,
{
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        (**self).layout(constraints)
    }

    fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
        (**self).paint(context, origin);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        (**self).handle_event(context, event)
    }

    fn accepts_focus(&self) -> bool {
        (**self).accepts_focus()
    }

    fn hit_test_child(&self, position: Point) -> Option<(usize, Point)> {
        (**self).hit_test_child(position)
    }

    fn event_child(&mut self, index: usize) -> Option<&mut (dyn Widget + '_)> {
        (**self).event_child(index)
    }

    fn event_child_ref(&self, index: usize) -> Option<&(dyn Widget + '_)> {
        (**self).event_child_ref(index)
    }

    fn event_child_count(&self) -> usize {
        (**self).event_child_count()
    }

    fn event_child_origin(&self, index: usize) -> Option<Point> {
        (**self).event_child_origin(index)
    }
}
