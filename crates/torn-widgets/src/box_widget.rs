use torn_core::{Color, Constraints, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{ChildLayout, LayoutContext, LayoutResult, SurfaceStyle, UiEnvironment, Widget};

/// A single-child container similar to an HTML `div`.
///
/// Its child relationship is owned by [`torn_ui::UiRuntime`]. Append at most
/// one child to this node. The child is placed at the top-left corner, inset by
/// the optional surface padding.
#[derive(Default)]
pub struct Box {
    style: SurfaceStyle,
}

impl Box {
    /// Creates an empty transparent box.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            style: SurfaceStyle {
                background: None,
                border: None,
                corner_radius: None,
                padding: None,
            },
        }
    }

    /// Sets the optional background color.
    pub fn set_background(&mut self, color: Option<Color>) {
        self.style.background = color;
    }

    /// Replaces the box's visual style.
    pub fn set_style(&mut self, style: SurfaceStyle) {
        self.style = style;
    }

    /// Returns the box's local visual style.
    #[must_use]
    pub const fn style(&self) -> SurfaceStyle {
        self.style
    }

    /// Returns this box with `color` as its background.
    #[must_use]
    pub fn with_background(mut self, color: Color) -> Self {
        self.style.background = Some(color);
        self
    }

    /// Returns this box with a border.
    #[must_use]
    pub fn with_border(mut self, border: torn_ui::Border) -> Self {
        self.style.border = Some(border);
        self
    }

    /// Returns this box with rounded corners of `radius` logical pixels.
    #[must_use]
    pub fn with_corner_radius(mut self, radius: f32) -> Self {
        self.style.corner_radius = Some(radius);
        self
    }

    /// Returns this box with an inset between its edge and direct child.
    #[must_use]
    pub fn with_padding(mut self, padding: torn_core::Insets) -> Self {
        self.style.padding = Some(padding);
        self
    }
}

impl Widget for Box {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let padding = self.style.padding.unwrap_or(torn_core::Insets::ZERO);
        let max = constraints.max();
        let child_max = size(
            (max.width() - padding.horizontal()).max(0.0),
            (max.height() - padding.vertical()).max(0.0),
        );
        let mut children = Vec::new();
        let content_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(id, Point::new(padding.left, padding.top)));
            layout.size()
        } else {
            Size::ZERO
        };
        LayoutResult::with_children(
            constraints.constrain(size(
                content_size.width() + padding.horizontal(),
                content_size.height() + padding.vertical(),
            )),
            children,
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let radius = self.style.corner_radius.unwrap_or(0.0);
        if let Some(background) = self.style.background {
            if radius == 0.0 {
                context.fill_rect(bounds, background);
            } else {
                context.fill_rounded_rect(bounds, radius, background);
            }
        }
        if let Some(border) = self.style.border {
            if radius == 0.0 {
                context.stroke_rect(bounds, border.width, border.color);
            } else {
                context.stroke_rounded_rect(bounds, radius, border.width, border.color);
            }
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("box layout sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Constraints};
    use torn_render::{DisplayCommand, DisplayList, PaintContext};
    use torn_ui::{Border, UiRuntime};

    use super::Box;

    #[test]
    fn paints_a_rounded_surface_before_its_border() {
        let background = Color::rgba8(10, 20, 30, 255);
        let border = Border::new(2.0, Color::rgba8(40, 50, 60, 255));
        let mut runtime = UiRuntime::new(
            Box::new()
                .with_background(background)
                .with_corner_radius(6.0)
                .with_border(border),
        );
        let mut display_list = DisplayList::new();

        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("box layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("box paint succeeds");

        assert!(matches!(
            display_list.commands(),
            [
                DisplayCommand::FillRoundedRect {
                    radius: 6.0,
                    color,
                    ..
                },
                DisplayCommand::StrokeRoundedRect {
                    radius: 6.0,
                    width: 2.0,
                    color: border_color,
                    ..
                }
            ] if *color == background && *border_color == border.color
        ));
    }
}
