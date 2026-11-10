use torn_core::{Color, Constraints, Insets, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, LayoutContext, LayoutResult, StyleTarget, TssStyle, UiEnvironment, Widget,
};

const MIN_HEIGHT: f32 = 20.0;
const DOT_SIZE: f32 = 8.0;

/// Semantic color family used by a [`Badge`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BadgeTone {
    /// Neutral surface with the theme's normal foreground.
    #[default]
    Neutral,
    /// Accent-colored badge for counts and highlights.
    Accent,
    /// Positive or completed state.
    Success,
    /// Caution or pending state.
    Warning,
    /// Error or destructive state.
    Danger,
}

/// A small pill-shaped surface for labels, counts, and status indicators.
///
/// Append at most one child to supply the badge content. A badge without a
/// child paints a compact colored dot.
pub struct Badge {
    tone: BadgeTone,
    padding: Insets,
    corner_radius: Option<f32>,
    background: Option<Color>,
    size: Size,
}

impl Badge {
    /// Creates a neutral badge with compact horizontal padding.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tone: BadgeTone::Neutral,
            padding: Insets::new(2.0, 8.0, 2.0, 8.0),
            corner_radius: None,
            background: None,
            size: Size::ZERO,
        }
    }

    /// Returns this badge with a semantic tone.
    #[must_use]
    pub const fn with_tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }

    /// Returns this badge with explicit content padding.
    #[must_use]
    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    /// Returns this badge with an explicit corner radius.
    #[must_use]
    pub const fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    /// Returns this badge with an explicit background color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns the semantic tone.
    #[must_use]
    pub const fn tone(&self) -> BadgeTone {
        self.tone
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Badge {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let content_max = size(
            (max.width() - self.padding.horizontal()).max(0.0),
            (max.height() - self.padding.vertical()).max(0.0),
        );
        let mut children = Vec::new();
        let badge_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(content_max))
                .expect("runtime child index is valid");
            let child_size = layout.size();
            let inner_height = child_size.height().max(MIN_HEIGHT);
            let y = self.padding.top + ((inner_height - child_size.height()) / 2.0).max(0.0);
            children.push(ChildLayout::new(id, Point::new(self.padding.left, y)));
            size(
                child_size.width() + self.padding.horizontal(),
                inner_height + self.padding.vertical(),
            )
        } else {
            size(DOT_SIZE, DOT_SIZE)
        };
        self.size = constraints.constrain(badge_size);
        LayoutResult::with_children(self.size, children)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("badge")
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
        let tone_background = match self.tone {
            BadgeTone::Neutral => theme.control_background(),
            BadgeTone::Accent => theme.accent(),
            BadgeTone::Success => theme.success(),
            BadgeTone::Warning => theme.warning(),
            BadgeTone::Danger => theme.danger(),
        };
        let background = self
            .background
            .or(style.background)
            .unwrap_or(tone_background);
        let radius = self
            .corner_radius
            .or(style.corner_radius)
            .unwrap_or(bounds.size.height() / 2.0)
            .min(bounds.size.height() / 2.0);
        context.fill_rounded_rect(bounds, radius, background);
        if let Some(border) = style.border {
            context.stroke_rounded_rect(bounds, radius, border.width, border.color);
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("badge sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn paints_a_pill_with_the_theme_accent() {
        let mut runtime = UiRuntime::new(Badge::new().with_tone(BadgeTone::Accent));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(30.0, 20.0)).expect("valid constraints"))
            .expect("badge layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("badge paint succeeds");

        assert!(matches!(
            display_list.commands(),
            [DisplayCommand::FillRoundedRect { radius, .. }]
                if (radius - 10.0).abs() <= f32::EPSILON
        ));
    }
}
