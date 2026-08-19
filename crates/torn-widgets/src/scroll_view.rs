use torn_core::{Constraints, InputEvent, Point, Size, WheelDelta};
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Widget,
};

/// Scroll direction supported by [`ScrollView`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScrollAxis {
    /// Scroll horizontally.
    Horizontal,
    /// Scroll vertically.
    #[default]
    Vertical,
}

/// A clipped single-child viewport scrollable with the pointer wheel.
pub struct ScrollView {
    axis: ScrollAxis,
    offset: f32,
    viewport: f32,
    content: f32,
}

impl ScrollView {
    /// Creates a scroll view along `axis`.
    #[must_use]
    pub const fn new(axis: ScrollAxis) -> Self {
        Self {
            axis,
            offset: 0.0,
            viewport: 0.0,
            content: 0.0,
        }
    }

    /// Creates a vertically scrolling view.
    #[must_use]
    pub const fn vertical() -> Self {
        Self::new(ScrollAxis::Vertical)
    }

    /// Creates a horizontally scrolling view.
    #[must_use]
    pub const fn horizontal() -> Self {
        Self::new(ScrollAxis::Horizontal)
    }

    /// Returns the current scroll offset in logical pixels.
    #[must_use]
    pub const fn offset(&self) -> f32 {
        self.offset
    }

    /// Sets the scroll offset, clamping it to the known content bounds.
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset.clamp(0.0, (self.content - self.viewport).max(0.0));
    }
}

impl Widget for ScrollView {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let viewport = constraints.constrain(size(
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
        self.viewport = axis_length(viewport, self.axis);
        let child_constraints = match self.axis {
            ScrollAxis::Horizontal => Constraints::loose(size(f32::INFINITY, viewport.height())),
            ScrollAxis::Vertical => Constraints::loose(size(viewport.width(), f32::INFINITY)),
        };
        let mut children = Vec::new();
        if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, child_constraints)
                .expect("runtime child index is valid");
            self.content = axis_length(layout.size(), self.axis);
            self.set_offset(self.offset);
            let origin = match self.axis {
                ScrollAxis::Horizontal => Point::new(-self.offset, 0.0),
                ScrollAxis::Vertical => Point::new(0.0, -self.offset),
            };
            children.push(ChildLayout::new(id, origin));
        } else {
            self.content = 0.0;
            self.offset = 0.0;
        }
        LayoutResult::with_children(viewport, children)
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Bubble {
            return EventStatus::Ignored;
        }
        let InputEvent::Wheel(wheel) = event else {
            return EventStatus::Ignored;
        };
        let delta = match wheel.delta {
            WheelDelta::Pixels(delta) => axis_coordinate(delta, self.axis),
            WheelDelta::Lines(delta) => axis_coordinate(delta, self.axis) * 40.0,
        };
        let old = self.offset;
        self.set_offset(self.offset - delta);
        if (self.offset - old).abs() <= f32::EPSILON {
            return EventStatus::Ignored;
        }
        context.request_redraw();
        EventStatus::Handled
    }

    fn clips_children(&self) -> bool {
        true
    }
}

fn axis_length(size: Size, axis: ScrollAxis) -> f32 {
    match axis {
        ScrollAxis::Horizontal => size.width(),
        ScrollAxis::Vertical => size.height(),
    }
}

fn axis_coordinate(point: Point, axis: ScrollAxis) -> f32 {
    match axis {
        ScrollAxis::Horizontal => point.x,
        ScrollAxis::Vertical => point.y,
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("scroll layout sizes are non-negative and not NaN")
}
