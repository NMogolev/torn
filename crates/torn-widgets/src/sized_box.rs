use torn_core::{Constraints, Point, Size};
use torn_ui::{ChildLayout, LayoutContext, LayoutResult, Widget};

/// A single-child container with optional explicit dimensions.
pub struct SizedBox {
    width: Option<f32>,
    height: Option<f32>,
}

impl SizedBox {
    /// Creates a box with optional non-negative dimensions.
    ///
    /// Negative or NaN values are ignored so the widget remains valid.
    #[must_use]
    pub fn new(width: Option<f32>, height: Option<f32>) -> Self {
        Self {
            width: width.filter(|value| value.is_finite() && *value >= 0.0),
            height: height.filter(|value| value.is_finite() && *value >= 0.0),
        }
    }
}

impl Widget for SizedBox {
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
            children.push(ChildLayout::new(id, Point::ZERO));
            layout.size()
        } else {
            Size::ZERO
        };
        let target = constraints.constrain(size(
            self.width.unwrap_or(child_size.width()),
            self.height.unwrap_or(child_size.height()),
        ));
        LayoutResult::with_children(target, children)
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("sized box dimensions are valid")
}
