use torn_core::{Color, Constraints, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{ChildLayout, LayoutContext, LayoutResult, Widget};

/// A single-child container similar to an HTML `div`.
///
/// Its child relationship is owned by [`torn_ui::UiRuntime`]. Append at most
/// one child to this node, then the child is placed at the top-left corner.
#[derive(Default)]
pub struct Box {
    background: Option<Color>,
    size: Size,
}

impl Box {
    /// Creates an empty transparent box.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            background: None,
            size: Size::ZERO,
        }
    }

    /// Sets the optional background color.
    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }
}

impl Widget for Box {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let mut children = Vec::new();
        let content_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(constraints.max()))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(id, Point::ZERO));
            layout.size()
        } else {
            Size::ZERO
        };
        self.size = constraints.constrain(content_size);
        LayoutResult::with_children(self.size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
        if let Some(background) = self.background {
            context.fill_rect(Rect::new(origin, self.size), background);
        }
    }
}
