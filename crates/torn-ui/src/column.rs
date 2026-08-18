use torn_core::{Constraints, Point, Size};

use crate::{ChildLayout, LayoutContext, LayoutResult, Widget};

/// A container that positions its runtime-owned children top-to-bottom.
///
/// Children receive the column's maximum width and an unbounded height. The
/// column uses the largest child width and the sum of child heights, then
/// constrains that size to its own constraints.
#[derive(Default)]
pub struct Column;

impl Column {
    /// Creates a column without children.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Widget for Column {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let child_constraints = Constraints::loose(size(constraints.max().width(), f32::INFINITY));
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut children = Vec::with_capacity(context.child_count());

        for index in 0..context.child_count() {
            let (id, result) = context
                .layout_child(index, child_constraints)
                .expect("runtime child index is valid");
            let child_size = result.size();
            children.push(ChildLayout::new(id, Point::new(0.0, height)));
            width = width.max(child_size.width());
            height += child_size.height();
        }

        LayoutResult::with_children(constraints.constrain(size(width, height)), children)
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("column layout sizes are non-negative and not NaN")
}
