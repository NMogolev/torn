use torn_core::{Constraints, Point, Size};
use torn_ui::{ChildLayout, LayoutContext, LayoutResult, Widget};

/// Placement along a single layout axis.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Alignment {
    /// Place content at the start edge.
    Start,
    /// Place content at the center.
    #[default]
    Center,
    /// Place content at the end edge.
    End,
}

/// A single-child container that aligns its child inside the available area.
pub struct Align {
    horizontal: Alignment,
    vertical: Alignment,
}

impl Align {
    /// Creates an alignment container.
    #[must_use]
    pub const fn new(horizontal: Alignment, vertical: Alignment) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}

impl Default for Align {
    fn default() -> Self {
        Self::new(Alignment::Center, Alignment::Center)
    }
}

impl Widget for Align {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let mut children = Vec::new();
        let child_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(constraints.max()))
                .expect("runtime child index is valid");
            let child_size = layout.size();
            let size = constraints.constrain(child_size);
            children.push(ChildLayout::new(
                id,
                Point::new(
                    offset(size.width(), child_size.width(), self.horizontal),
                    offset(size.height(), child_size.height(), self.vertical),
                ),
            ));
            return LayoutResult::with_children(size, children);
        } else {
            Size::ZERO
        };
        LayoutResult::with_children(constraints.constrain(child_size), children)
    }
}

fn offset(available: f32, content: f32, alignment: Alignment) -> f32 {
    match alignment {
        Alignment::Start => 0.0,
        Alignment::Center => ((available - content) / 2.0).max(0.0),
        Alignment::End => (available - content).max(0.0),
    }
}
