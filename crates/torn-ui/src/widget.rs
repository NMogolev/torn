use torn_core::{Constraints, InputEvent, Rect};
use torn_render::PaintContext;

use crate::{EventContext, EventStatus, LayoutContext, LayoutResult, UiEnvironment};

/// A retained UI element whose placement and relationships are owned by the runtime.
///
/// A widget defines its own behavior. Its direct children are retained by
/// [`crate::UiRuntime`], and can be measured through [`LayoutContext`] during
/// layout. The returned [`LayoutResult`] must position every direct child once.
pub trait Widget {
    /// Lays out this widget and positions its direct runtime-owned children.
    fn layout(&mut self, context: &mut LayoutContext<'_>, constraints: Constraints)
    -> LayoutResult;

    /// Records this widget's own paint operations within its runtime-owned `bounds`.
    ///
    /// The runtime paints descendants after this method returns.
    fn paint(&self, _context: &mut PaintContext<'_>, _environment: &UiEnvironment, _bounds: Rect) {}

    /// Handles an input event during capture, target, or bubble propagation.
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

    /// Returns whether the runtime clips all descendant painting to this widget's bounds.
    fn clips_children(&self) -> bool {
        false
    }
}

impl<W> Widget for Box<W>
where
    W: Widget + ?Sized,
{
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        (**self).layout(context, constraints)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        (**self).paint(context, environment, bounds);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        (**self).handle_event(context, event)
    }

    fn accepts_focus(&self) -> bool {
        (**self).accepts_focus()
    }

    fn clips_children(&self) -> bool {
        (**self).clips_children()
    }
}
