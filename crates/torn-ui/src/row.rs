use torn_core::{Constraints, Point, Size};

use crate::{ChildLayout, LayoutContext, LayoutResult, Widget};

/// A container that positions its runtime-owned children left-to-right.
///
/// Children receive an unbounded width and the row's maximum height. The row
/// uses the sum of child widths and the largest child height, then constrains
/// that size to its own constraints.
#[derive(Default)]
pub struct Row;

impl Row {
    /// Creates a row without children.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Widget for Row {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let child_constraints = Constraints::loose(size(f32::INFINITY, constraints.max().height()));
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut children = Vec::with_capacity(context.child_count());

        for index in 0..context.child_count() {
            let (id, result) = context
                .layout_child(index, child_constraints)
                .expect("runtime child index is valid");
            let child_size = result.size();
            children.push(ChildLayout::new(id, Point::new(width, 0.0)));
            width += child_size.width();
            height = height.max(child_size.height());
        }

        LayoutResult::with_children(constraints.constrain(size(width, height)), children)
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("row layout sizes are non-negative and not NaN")
}
