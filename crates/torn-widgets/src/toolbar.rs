use torn_core::{Constraints, Insets, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{ChildLayout, LayoutContext, LayoutResult, UiEnvironment, Widget};

const DEFAULT_PADDING: f32 = 4.0;

/// A horizontal application toolbar for runtime-owned quick-action controls.
///
/// Append buttons, icons, or other controls as direct children. The toolbar
/// positions them from left to right with the theme's standard spacing.
pub struct Toolbar {
    padding: Insets,
}

impl Toolbar {
    /// Creates a toolbar with four logical pixels of padding on every edge.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            padding: Insets::all(DEFAULT_PADDING),
        }
    }

    /// Returns this toolbar with explicit content padding.
    #[must_use]
    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    /// Returns the padding around the quick-action controls.
    #[must_use]
    pub const fn padding(&self) -> Insets {
        self.padding
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Toolbar {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let child_max = size(
            (max.width() - self.padding.horizontal()).max(0.0),
            (max.height() - self.padding.vertical()).max(0.0),
        );
        let gap = context.environment().theme().spacing();
        let mut cursor = self.padding.left;
        let mut content_height = 0.0_f32;
        let mut children = Vec::with_capacity(context.child_count());
        for index in 0..context.child_count() {
            let (id, layout) = context
                .layout_child(index, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            let child_size = layout.size();
            children.push(ChildLayout::new(id, Point::new(cursor, self.padding.top)));
            cursor += child_size.width() + gap;
            content_height = content_height.max(child_size.height());
        }
        let content_width = if context.child_count() == 0 {
            0.0
        } else {
            cursor - self.padding.left - gap
        };
        let measured = size(
            content_width + self.padding.horizontal(),
            content_height + self.padding.vertical(),
        );
        LayoutResult::with_children(constraints.constrain(measured), children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, environment.theme().button_background());
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("toolbar sizes are non-negative and not NaN")
}
