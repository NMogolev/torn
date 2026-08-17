use torn_core::Constraints;

use crate::LayoutResult;

/// A retained UI element that can determine its size and place its children.
///
/// Implementations must return a finite size within `constraints`. Child bounds
/// in the returned [`LayoutResult`] are relative to this widget's origin.
pub trait Widget {
    /// Lays out this widget and its descendants within `constraints`.
    fn layout(&mut self, constraints: Constraints) -> LayoutResult;
}

impl<W> Widget for Box<W>
where
    W: Widget + ?Sized,
{
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        (**self).layout(constraints)
    }
}
