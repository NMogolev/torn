use torn_core::{Constraints, InputEvent, Point, PointerButton, PointerId, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    StyleTarget, TssStyle, UiEnvironment, Widget,
};

/// Direction in which a [`Splitter`] divides its two children.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SplitAxis {
    /// Arrange children left-to-right with a vertical divider.
    #[default]
    Horizontal,
    /// Arrange children top-to-bottom with a horizontal divider.
    Vertical,
}

/// A two-pane container with a pointer-draggable divider.
pub struct Splitter {
    axis: SplitAxis,
    ratio: f32,
    thickness: f32,
    dragging: Option<PointerId>,
    length: f32,
    resized: Signal<f32>,
}

impl Splitter {
    /// Creates a splitter whose first pane initially occupies `ratio` of the available space.
    #[must_use]
    pub fn new(axis: SplitAxis, ratio: f32) -> Self {
        Self {
            axis,
            ratio: ratio.clamp(0.0, 1.0),
            thickness: 4.0,
            dragging: None,
            length: 0.0,
            resized: Signal::new(),
        }
    }

    /// Returns the current first-pane ratio.
    #[must_use]
    pub const fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Returns a signal notified when pointer dragging changes the ratio.
    #[must_use]
    pub fn resized(&self) -> Signal<f32> {
        self.resized.clone()
    }
}

impl Widget for Splitter {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let size = constraints.constrain(valid_size(
            if max.width().is_finite() {
                max.width()
            } else {
                constraints.min().width()
            },
            if max.height().is_finite() {
                max.height()
            } else {
                constraints.min().height()
            },
        ));
        self.length = match self.axis {
            SplitAxis::Horizontal => size.width(),
            SplitAxis::Vertical => size.height(),
        };
        let content_length = (self.length - self.thickness).max(0.0);
        let first_length = content_length * self.ratio;
        let second_length = content_length - first_length;
        let mut children = Vec::new();
        for index in 0..context.child_count() {
            if index >= 2 {
                let (id, _) = context
                    .layout_child(
                        index,
                        Constraints::tight(Size::ZERO).expect("zero size is finite"),
                    )
                    .expect("runtime child index is valid");
                children.push(ChildLayout::with_visibility(id, Point::ZERO, false));
                continue;
            }
            let (child_size, origin) = match (self.axis, index) {
                (SplitAxis::Horizontal, 0) => {
                    (valid_size(first_length, size.height()), Point::ZERO)
                }
                (SplitAxis::Horizontal, _) => (
                    valid_size(second_length, size.height()),
                    Point::new(first_length + self.thickness, 0.0),
                ),
                (SplitAxis::Vertical, 0) => (valid_size(size.width(), first_length), Point::ZERO),
                (SplitAxis::Vertical, _) => (
                    valid_size(size.width(), second_length),
                    Point::new(0.0, first_length + self.thickness),
                ),
            };
            let (id, _) = context
                .layout_child(
                    index,
                    Constraints::tight(child_size).expect("split child size is finite"),
                )
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(id, origin));
        }
        LayoutResult::with_children(size, children)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("splitter")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        if let Some(background) = style.background {
            context.fill_rect(bounds, background);
        }
        let first = (self.length - self.thickness).max(0.0) * self.ratio;
        let divider = match self.axis {
            SplitAxis::Horizontal => rect(
                bounds.origin.x + first,
                bounds.origin.y,
                self.thickness,
                bounds.size.height(),
            ),
            SplitAxis::Vertical => rect(
                bounds.origin.x,
                bounds.origin.y + first,
                bounds.size.width(),
                self.thickness,
            ),
        };
        context.fill_rect(divider, style.accent_or(environment.theme().accent()));
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.divider_contains(pointer.position) =>
            {
                self.dragging = Some(pointer.pointer_id);
                context.capture_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) if self.dragging == Some(pointer.pointer_id) => {
                let coordinate = match self.axis {
                    SplitAxis::Horizontal => pointer.position.x,
                    SplitAxis::Vertical => pointer.position.y,
                };
                let content_length = (self.length - self.thickness).max(0.0);
                if content_length > 0.0 {
                    self.ratio = (coordinate / content_length).clamp(0.0, 1.0);
                    self.resized.emit(&self.ratio);
                    context.request_layout();
                }
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) | InputEvent::PointerCancel(pointer)
                if self.dragging == Some(pointer.pointer_id) =>
            {
                self.dragging = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }
}

impl Splitter {
    fn divider_contains(&self, point: Point) -> bool {
        let first = (self.length - self.thickness).max(0.0) * self.ratio;
        match self.axis {
            SplitAxis::Horizontal => point.x >= first && point.x < first + self.thickness,
            SplitAxis::Vertical => point.y >= first && point.y < first + self.thickness,
        }
    }
}

fn valid_size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("splitter layout sizes are non-negative and not NaN")
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(Point::new(x, y), valid_size(width, height))
}

#[cfg(test)]
mod tests {
    use torn_core::{Modifiers, PointerButtons, PointerEvent};
    use torn_render::{DisplayList, PaintContext};
    use torn_ui::UiRuntime;

    use crate::SizedBox;

    use super::*;

    #[test]
    fn dragging_relayouts_panes_before_the_next_paint() {
        let mut runtime = UiRuntime::new(Splitter::new(SplitAxis::Horizontal, 0.5));
        let first = runtime
            .append_child(runtime.root(), SizedBox::new(None, None))
            .expect("splitter exists");
        let second = runtime
            .append_child(runtime.root(), SizedBox::new(None, None))
            .expect("splitter exists");
        runtime
            .layout(Constraints::tight(valid_size(100.0, 40.0)).expect("canvas is finite"))
            .expect("initial layout succeeds");

        let down = InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(49.0, 20.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });
        let move_right = InputEvent::PointerMove(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(80.0, 20.0),
            button: None,
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(runtime.dispatch_event(&down), EventStatus::Handled);
        assert_eq!(runtime.dispatch_event(&move_right), EventStatus::Handled);
        assert!(runtime.last_layout().is_none());

        let mut display_list = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("deferred layout and paint succeed");
        assert_eq!(
            runtime.bounds(first),
            Some(Rect::new(Point::ZERO, valid_size(80.0, 40.0)))
        );
        assert_eq!(
            runtime.bounds(second),
            Some(Rect::new(Point::new(84.0, 0.0), valid_size(16.0, 40.0)))
        );
    }
}
