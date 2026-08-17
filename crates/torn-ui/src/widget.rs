use torn_core::{Constraints, InputEvent, Point};
use torn_render::PaintContext;

use crate::{EventStatus, LayoutResult};

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

    /// Handles an input event targeted at this widget.
    ///
    /// Pointer positions are relative to this widget's origin. The default
    /// implementation ignores the event, so widgets interested only in layout
    /// do not need an event handler.
    fn handle_event(&mut self, _event: &InputEvent) -> EventStatus {
        EventStatus::Ignored
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

    fn handle_event(&mut self, event: &InputEvent) -> EventStatus {
        (**self).handle_event(event)
    }
}
