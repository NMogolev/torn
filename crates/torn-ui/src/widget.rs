use torn_core::{Constraints, InputEvent, Rect};
use torn_render::PaintContext;

use crate::{
    EventContext, EventStatus, LayoutContext, LayoutResult, StyleTarget, TssStyle, UiEnvironment,
};

/// A retained UI element whose placement and relationships are owned by the runtime.
///
/// A widget defines its own behavior. Its direct children are retained by
/// [`crate::UiRuntime`], and visible children can be measured through
/// [`LayoutContext`] during layout. Children omitted from the returned
/// [`LayoutResult`] remain retained but are excluded from painting, hit testing,
/// and focus traversal. This permits virtualized containers to measure only the
/// currently visible range.
pub trait Widget {
    /// Lays out this widget and positions its direct runtime-owned children.
    fn layout(&mut self, context: &mut LayoutContext<'_>, constraints: Constraints)
    -> LayoutResult;

    /// Returns the style metadata used to resolve this node's TSS cascade.
    ///
    /// Widgets normally return a [`StyleTarget`] with their type name. The
    /// runtime adds interaction states it owns, such as `:hover` and `:focus`,
    /// before resolving the sheet. Wrap a widget in [`crate::Styled`] when an
    /// application needs to assign an id or classes.
    fn style_target(&self) -> StyleTarget {
        StyleTarget::empty()
    }

    /// Records this widget's own paint operations within its runtime-owned `bounds`.
    ///
    /// The runtime paints descendants after this method returns.
    fn paint(&self, _context: &mut PaintContext<'_>, _environment: &UiEnvironment, _bounds: Rect) {}

    /// Records paint operations using the style resolved for this node.
    ///
    /// The default forwards to [`Widget::paint`] for widgets that only use the
    /// semantic theme. Styled standard widgets override this method so a single
    /// node can consume its resolved `background`, `border`, `padding`,
    /// `border-radius`, and other typed values.
    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let _ = style;
        self.paint(context, environment, bounds);
    }

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

    /// Returns whether this widget itself may be selected as a pointer hit-test target.
    ///
    /// Decorative widgets should return `false` so an interactive ancestor can
    /// receive pointer input while still owning the decoration as a child.
    fn participates_in_hit_test(&self) -> bool {
        true
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

    fn style_target(&self) -> StyleTarget {
        (**self).style_target()
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        (**self).paint(context, environment, bounds);
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        (**self).paint_styled(context, environment, bounds, style);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        (**self).handle_event(context, event)
    }

    fn accepts_focus(&self) -> bool {
        (**self).accepts_focus()
    }

    fn participates_in_hit_test(&self) -> bool {
        (**self).participates_in_hit_test()
    }

    fn clips_children(&self) -> bool {
        (**self).clips_children()
    }
}
