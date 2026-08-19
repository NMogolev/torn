use torn_core::{Constraints, Insets, Point, Size};
use torn_ui::{ChildLayout, LayoutContext, LayoutResult, Widget};

/// A single-child container that inserts empty space around its child.
pub struct Padding {
    insets: Insets,
}

impl Padding {
    /// Creates padding with `insets`.
    #[must_use]
    pub const fn new(insets: Insets) -> Self {
        Self { insets }
    }

    /// Returns the configured insets.
    #[must_use]
    pub const fn insets(&self) -> Insets {
        self.insets
    }

    /// Replaces the configured insets.
    pub fn set_insets(&mut self, insets: Insets) {
        self.insets = insets;
    }
}

impl Widget for Padding {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let child_max = size(
            (max.width() - self.insets.horizontal()).max(0.0),
            (max.height() - self.insets.vertical()).max(0.0),
        );
        let mut children = Vec::new();
        let child_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(
                id,
                Point::new(self.insets.left, self.insets.top),
            ));
            layout.size()
        } else {
            Size::ZERO
        };
        LayoutResult::with_children(
            constraints.constrain(size(
                child_size.width() + self.insets.horizontal(),
                child_size.height() + self.insets.vertical(),
            )),
            children,
        )
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("padding layout sizes are non-negative and not NaN")
}
