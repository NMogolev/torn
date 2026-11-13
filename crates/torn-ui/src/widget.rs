use std::time::Duration;

use torn_core::{Constraints, InputEvent, Rect};
use torn_render::PaintContext;

use crate::{
    Accessibility, AccessibilityAction, AccessibilityRole, EventContext, EventStatus,
    LayoutContext, LayoutResult, StyleTarget, TssStyle, UiEnvironment,
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

    /// Advances widget-owned, time-driven content by `delta`.
    ///
    /// The runtime calls this before painting every frame. Widgets that play
    /// media, drive procedural effects, or otherwise change visually with time
    /// should update their state here and return whether paint output changed.
    /// `settle` is `true` when the host requested an immediate jump to the
    /// final state, such as when animations are disabled.
    ///
    /// Returning `true` invalidates the retained display list so the next
    /// paint reflects the new state. This method must not emit application
    /// signals; report user-driven changes from [`Widget::handle_event`].
    fn advance_animation(&mut self, _delta: Duration, _settle: bool) -> bool {
        false
    }

    /// Returns whether this widget needs continuous animation frames.
    ///
    /// Platform adapters use this through [`crate::UiRuntime::next_frame_in`]
    /// to schedule the next paint while media is playing.
    fn is_animating(&self) -> bool {
        false
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

    /// Describes this widget to assistive technologies.
    ///
    /// The runtime adds focus state and absolute bounds. Widgets should report
    /// their role, accessible name, value, and interaction states. Returning
    /// the default [`AccessibilityRole::GenericContainer`] keeps the node in
    /// the tree as a structural container.
    fn accessibility(&self) -> Accessibility {
        Accessibility::new(AccessibilityRole::GenericContainer)
    }

    /// Returns this widget's position in the sequential tab order.
    ///
    /// Positive values are visited before zero or unspecified values. A
    /// negative index removes the widget from Tab traversal while leaving it
    /// available to [`crate::UiRuntime::focus_widget`].
    fn tab_index(&self) -> Option<i32> {
        None
    }

    /// Performs an action requested by an assistive technology.
    ///
    /// The runtime validates focus requests itself. Returning `true` means the
    /// widget handled the action and the runtime should invalidate its layout
    /// and paint output.
    fn accessibility_action(&mut self, _action: &AccessibilityAction) -> bool {
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

    fn advance_animation(&mut self, delta: Duration, settle: bool) -> bool {
        (**self).advance_animation(delta, settle)
    }

    fn is_animating(&self) -> bool {
        (**self).is_animating()
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        (**self).handle_event(context, event)
    }

    fn accepts_focus(&self) -> bool {
        (**self).accepts_focus()
    }

    fn accessibility(&self) -> Accessibility {
        (**self).accessibility()
    }

    fn tab_index(&self) -> Option<i32> {
        (**self).tab_index()
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        (**self).accessibility_action(action)
    }

    fn participates_in_hit_test(&self) -> bool {
        (**self).participates_in_hit_test()
    }

    fn clips_children(&self) -> bool {
        (**self).clips_children()
    }
}
