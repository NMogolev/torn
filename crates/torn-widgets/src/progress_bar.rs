use torn_core::{Constraints, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{LayoutContext, LayoutResult, StyleTarget, TssStyle, UiEnvironment, Widget};

const DEFAULT_WIDTH: f32 = 120.0;
const HEIGHT: f32 = 12.0;

/// A passive horizontal indicator for progress in the inclusive `0.0..=1.0` range.
pub struct ProgressBar {
    progress: f32,
}

impl ProgressBar {
    /// Creates an empty progress indicator.
    #[must_use]
    pub const fn new() -> Self {
        Self { progress: 0.0 }
    }

    /// Creates a progress indicator with `progress` clamped to `0.0..=1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `progress` is non-finite.
    #[must_use]
    pub fn with_progress(progress: f32) -> Self {
        let mut result = Self::new();
        result.set_progress(progress);
        result
    }

    /// Returns the current normalized progress.
    #[must_use]
    pub const fn progress(&self) -> f32 {
        self.progress
    }

    /// Sets normalized progress, clamping it to `0.0..=1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `progress` is non-finite.
    pub fn set_progress(&mut self, progress: f32) {
        assert!(progress.is_finite(), "progress must be finite");
        self.progress = progress.clamp(0.0, 1.0);
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ProgressBar {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let width = if constraints.max().width().is_finite() {
            constraints.max().width()
        } else {
            DEFAULT_WIDTH
        };
        LayoutResult::new(constraints.constrain(size(width, HEIGHT)))
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("progress-bar")
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
        let radius = style
            .corner_radius
            .unwrap_or(bounds.size.height() / 2.0)
            .min(bounds.size.height() / 2.0);
        context.fill_rounded_rect(
            bounds,
            radius,
            style
                .background
                .unwrap_or_else(|| theme.control_background()),
        );
        if let Some(border) = style.border {
            context.stroke_rounded_rect(bounds, radius, border.width, border.color);
        }
        let filled_width = bounds.size.width() * self.progress;
        if filled_width > 0.0 {
            let filled = Rect::new(bounds.origin, size(filled_width, bounds.size.height()));
            context.fill_rounded_rect(
                filled,
                radius.min(filled_width / 2.0),
                style.accent.unwrap_or_else(|| theme.accent()),
            );
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("progress bar sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn paints_the_normalized_filled_portion() {
        let mut runtime = UiRuntime::new(ProgressBar::with_progress(0.25));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(80.0, HEIGHT)).expect("valid constraints"))
            .expect("progress bar layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("progress bar paint succeeds");

        assert!(matches!(
            display_list.commands(),
            [
                DisplayCommand::FillRoundedRect { .. },
                DisplayCommand::FillRoundedRect { rect, .. },
            ] if (rect.size.width() - 20.0).abs() <= f32::EPSILON
        ));
    }

    #[test]
    fn clamps_programmatic_progress() {
        let mut progress = ProgressBar::with_progress(2.0);
        assert!((progress.progress() - 1.0).abs() <= f32::EPSILON);
        progress.set_progress(-1.0);
        assert!(progress.progress().abs() <= f32::EPSILON);
    }
}
