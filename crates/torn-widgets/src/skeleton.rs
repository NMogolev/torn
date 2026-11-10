use torn_core::{Color, Constraints, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    Accessibility, AccessibilityRole, LayoutContext, LayoutResult, StyleTarget, TssStyle,
    UiEnvironment, Widget,
};

const DEFAULT_WIDTH: f32 = 160.0;
const DEFAULT_HEIGHT: f32 = 16.0;

/// A placeholder surface used while content is loading.
///
/// The skeleton paints a rounded block with a lighter band that sweeps across
/// it. Applications animate the band by advancing [`Self::phase`] and
/// requesting a redraw, or leave it static for a subdued placeholder.
pub struct Skeleton {
    width: f32,
    height: f32,
    corner_radius: Option<f32>,
    color: Option<Color>,
    highlight: Option<Color>,
    phase: f32,
}

impl Skeleton {
    /// Creates a placeholder with the default size.
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            corner_radius: None,
            color: None,
            highlight: None,
            phase: 0.0,
        }
    }

    /// Returns the requested width in logical pixels.
    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width
    }

    /// Returns the requested height in logical pixels.
    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height
    }

    /// Returns the normalized animation phase in `0.0..1.0`.
    #[must_use]
    pub const fn phase(&self) -> f32 {
        self.phase
    }

    /// Returns this skeleton with an explicit logical size.
    #[must_use]
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = width.max(0.0);
        self.height = height.max(0.0);
        self
    }

    /// Returns this skeleton with an explicit corner radius.
    #[must_use]
    pub const fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius.max(0.0));
        self
    }

    /// Returns this skeleton with an explicit base color.
    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Returns this skeleton with an explicit shimmer color.
    #[must_use]
    pub const fn with_highlight(mut self, color: Color) -> Self {
        self.highlight = Some(color);
        self
    }

    /// Sets the normalized animation phase, wrapping it into `0.0..1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `phase` is non-finite.
    pub fn set_phase(&mut self, phase: f32) {
        assert!(phase.is_finite(), "skeleton phase must be finite");
        self.phase = phase.rem_euclid(1.0);
    }

    /// Advances the animation phase by `delta`, wrapping it into `0.0..1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `delta` is non-finite.
    pub fn advance_phase(&mut self, delta: f32) {
        assert!(delta.is_finite(), "skeleton phase delta must be finite");
        self.set_phase(self.phase + delta);
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Skeleton {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(constraints.constrain(size(self.width, self.height)))
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("skeleton")
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
            .corner_radius
            .or(style.corner_radius)
            .unwrap_or_else(|| theme.corner_radius())
            .min(bounds.size.height() / 2.0);
        let base = self
            .color
            .or(style.background)
            .unwrap_or_else(|| theme.control_background());
        context.fill_rounded_rect(bounds, radius, base);

        if bounds.size.width() <= 0.0 || bounds.size.height() <= 0.0 {
            return;
        }
        let highlight = self
            .highlight
            .or(style.foreground)
            .unwrap_or_else(|| theme.foreground());
        let highlight = highlight.with_alpha(highlight.alpha * 0.10);
        let band_width = (bounds.size.width() * 0.35)
            .max(12.0)
            .min(bounds.size.width());
        let travel = bounds.size.width() + band_width;
        let band = Rect::new(
            Point::new(
                bounds.origin.x - band_width + self.phase * travel,
                bounds.origin.y,
            ),
            size(band_width, bounds.size.height()),
        );
        context.with_clip(bounds, |context| {
            context.fill_rounded_rect(band, radius, highlight);
        });
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::GenericContainer);
        accessibility.hidden = true;
        accessibility
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("skeleton sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn paints_a_rounded_base_and_a_clipped_highlight_band() {
        let mut runtime = UiRuntime::new(Skeleton::new().with_size(80.0, 12.0));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(80.0, 12.0)).expect("valid constraints"))
            .expect("skeleton layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("skeleton paint succeeds");

        assert!(matches!(
            display_list.commands(),
            [
                DisplayCommand::FillRoundedRect { .. },
                DisplayCommand::Save,
                DisplayCommand::PushClip { .. },
                DisplayCommand::FillRoundedRect { .. },
                DisplayCommand::Restore,
            ]
        ));
    }

    #[test]
    fn phase_moves_the_highlight_band_across_the_surface() {
        let mut skeleton = Skeleton::new().with_size(120.0, 12.0);
        skeleton.set_phase(0.5);
        assert!((skeleton.phase() - 0.5).abs() <= f32::EPSILON);

        let mut runtime = UiRuntime::new(skeleton);
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(120.0, 12.0)).expect("valid constraints"))
            .expect("skeleton layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("skeleton paint succeeds");

        let bands = display_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::FillRoundedRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(bands.len(), 2);
        assert!(bands[1].origin.x > bands[0].origin.x);
    }
}
