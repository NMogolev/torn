use torn_core::{Color, Constraints, Insets, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, LayoutContext, LayoutResult, StyleTarget, SurfaceStyle, TssStyle, UiEnvironment,
    Widget,
};

const DEFAULT_ELEVATION: f32 = 6.0;
const SHADOW_LAYERS: [(f32, f32, f32); 3] =
    [(0.60, 0.50, 0.10), (0.35, 0.30, 0.16), (0.15, 0.12, 0.24)];

/// Locally overridable visual properties for a [`Card`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CardStyle {
    /// Surface fill, border, corner radius, and padding.
    pub surface: SurfaceStyle,
    /// Shadow elevation in logical pixels. Zero disables the shadow.
    pub elevation: f32,
    /// Base shadow color. Its alpha is scaled by each shadow layer.
    pub shadow_color: Option<Color>,
}

/// A raised single-child surface for grouping related content.
///
/// Cards combine a rounded surface, optional border, padding, and a soft
/// multi-layer shadow. They are decorative containers and do not receive
/// pointer input themselves.
pub struct Card {
    style: CardStyle,
}

impl Card {
    /// Creates a card with a soft shadow and twelve pixels of padding.
    #[must_use]
    pub fn new() -> Self {
        Self {
            style: CardStyle {
                surface: SurfaceStyle {
                    background: None,
                    border: None,
                    corner_radius: None,
                    padding: Some(Insets::all(12.0)),
                },
                elevation: DEFAULT_ELEVATION,
                shadow_color: None,
            },
        }
    }

    /// Replaces the card's local visual style.
    pub fn set_style(&mut self, style: CardStyle) {
        self.style = style;
    }

    /// Returns the card's local visual style.
    #[must_use]
    pub const fn style(&self) -> CardStyle {
        self.style
    }

    /// Returns this card with `style` applied locally.
    #[must_use]
    pub fn with_style(mut self, style: CardStyle) -> Self {
        self.set_style(style);
        self
    }

    /// Returns this card with an explicit background color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.style.surface.background = Some(color);
        self
    }

    /// Returns this card with a border.
    #[must_use]
    pub const fn with_border(mut self, border: torn_ui::Border) -> Self {
        self.style.surface.border = Some(border);
        self
    }

    /// Returns this card with rounded corners of `radius` logical pixels.
    #[must_use]
    pub const fn with_corner_radius(mut self, radius: f32) -> Self {
        self.style.surface.corner_radius = Some(radius);
        self
    }

    /// Returns this card with an inset between its edge and direct child.
    #[must_use]
    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.style.surface.padding = Some(padding);
        self
    }

    /// Returns this card with an explicit shadow elevation.
    #[must_use]
    pub const fn with_elevation(mut self, elevation: f32) -> Self {
        self.style.elevation = elevation.max(0.0);
        self
    }

    /// Returns this card with an explicit shadow color.
    #[must_use]
    pub const fn with_shadow_color(mut self, color: Color) -> Self {
        self.style.shadow_color = Some(color);
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Card {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let padding = self
            .style
            .surface
            .padding
            .or(style.padding)
            .unwrap_or(Insets::all(12.0));
        let max = constraints.max();
        let child_max = size(
            (max.width() - padding.horizontal()).max(0.0),
            (max.height() - padding.vertical()).max(0.0),
        );
        let mut children = Vec::new();
        let child_size = if context.child_count() == 1 {
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
                child_size.width() + padding.horizontal(),
                child_size.height() + padding.vertical(),
            )),
            children,
        )
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("card")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let theme = environment.theme();
        let radius = self
            .style
            .surface
            .corner_radius
            .or(style.corner_radius)
            .unwrap_or_else(|| theme.corner_radius());
        let elevation = self.style.elevation;
        if elevation > 0.0 {
            let shadow = self
                .style
                .shadow_color
                .unwrap_or(Color::rgba(0.0, 0.0, 0.0, 0.55));
            for (spread, offset, alpha) in SHADOW_LAYERS {
                let spread = elevation * spread;
                let shadow_bounds = Rect::new(
                    Point::new(
                        bounds.origin.x - spread,
                        bounds.origin.y - spread + elevation * offset,
                    ),
                    size(
                        bounds.size.width() + spread * 2.0,
                        bounds.size.height() + spread * 2.0,
                    ),
                );
                context.fill_rounded_rect(
                    shadow_bounds,
                    radius + spread,
                    Color::rgba(shadow.red, shadow.green, shadow.blue, shadow.alpha * alpha),
                );
            }
        }
        let background = self
            .style
            .surface
            .background
            .or(style.background)
            .unwrap_or_else(|| theme.control_background());
        context.fill_rounded_rect(bounds, radius, background);
        if let Some(border) = self.style.surface.border.or(style.border) {
            context.stroke_rounded_rect(bounds, radius, border.width, border.color);
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("card sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn paints_shadow_layers_before_the_card_surface() {
        let mut runtime = UiRuntime::new(Card::new().with_elevation(4.0));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(80.0, 40.0)).expect("valid constraints"))
            .expect("card layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("card paint succeeds");

        let commands = display_list.commands();
        assert_eq!(commands.len(), 4);
        assert!(matches!(
            commands.last(),
            Some(DisplayCommand::FillRoundedRect { .. })
        ));
    }
}
