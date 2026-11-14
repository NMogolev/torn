use drawen_core::{Constraints, Point, Size};
use drawen_ui::{ChildLayout, LayoutContext, LayoutResult, StyleTarget, Widget};

/// A container that overlays all of its children at the top-left corner.
#[derive(Default)]
pub struct Stack;

impl Stack {
    /// Creates an empty stack.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Widget for Stack {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("stack")
    }

    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let mut size = Size::ZERO;
        let mut children = Vec::with_capacity(context.child_count());
        for index in 0..context.child_count() {
            let (id, layout) = context
                .layout_child(index, Constraints::loose(constraints.max()))
                .expect("runtime child index is valid");
            size = valid_size(
                size.width().max(layout.size().width()),
                size.height().max(layout.size().height()),
            );
            children.push(ChildLayout::new(id, Point::ZERO));
        }
        LayoutResult::with_children(constraints.constrain(size), children)
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

fn valid_size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("stack layout sizes are non-negative and not NaN")
}
