use std::f32::consts::TAU;

use drawen_core::{Affine, Color, Constraints, Point, Rect, Size};
use drawen_render::PaintContext;
use drawen_ui::{
    Accessibility, AccessibilityRole, DssStyle, LayoutContext, LayoutResult, StyleTarget,
    UiEnvironment, Widget,
};

const DEFAULT_DIAMETER: f32 = 24.0;
const DEFAULT_SEGMENTS: usize = 8;
const MIN_SEGMENTS: usize = 2;

/// A circular activity indicator for determinate or indeterminate progress.
///
/// The spinner paints a ring of short rounded segments. Determinate progress
/// highlights the completed portion of the ring and leaves a faint track;
/// indeterminate progress fades segments around the ring using [`Self::phase`]
/// as a normalized rotation.
pub struct Spinner {
    diameter: f32,
    segments: usize,
    progress: Option<f32>,
    phase: f32,
    color: Option<Color>,
    track_color: Option<Color>,
}

impl Spinner {
    /// Creates an indeterminate spinner with the default diameter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            diameter: DEFAULT_DIAMETER,
            segments: DEFAULT_SEGMENTS,
            progress: None,
            phase: 0.0,
            color: None,
            track_color: None,
        }
    }

    /// Creates a spinner showing `progress` clamped to `0.0..=1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `progress` is non-finite.
    #[must_use]
    pub fn with_progress(progress: f32) -> Self {
        let mut spinner = Self::new();
        spinner.set_progress(Some(progress));
        spinner
    }

    /// Returns the outer diameter in logical pixels.
    #[must_use]
    pub const fn diameter(&self) -> f32 {
        self.diameter
    }

    /// Returns the number of painted segments.
    #[must_use]
    pub const fn segments(&self) -> usize {
        self.segments
    }

    /// Returns the normalized progress value, or `None` when indeterminate.
    #[must_use]
    pub const fn progress(&self) -> Option<f32> {
        self.progress
    }

    /// Returns the normalized animation phase in `0.0..1.0`.
    #[must_use]
    pub const fn phase(&self) -> f32 {
        self.phase
    }

    /// Returns this spinner with an explicit outer diameter.
    #[must_use]
    pub fn with_diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(0.0);
        self
    }

    /// Returns this spinner with at least two painted segments.
    #[must_use]
    pub fn with_segments(mut self, segments: usize) -> Self {
        self.segments = segments.max(MIN_SEGMENTS);
        self
    }

    /// Returns this spinner with an explicit segment color.
    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Returns this spinner with an explicit inactive track color.
    #[must_use]
    pub const fn with_track_color(mut self, color: Color) -> Self {
        self.track_color = Some(color);
        self
    }

    /// Makes the spinner determinate, or indeterminate when `progress` is `None`.
    ///
    /// # Panics
    ///
    /// Panics when `progress` is `Some` and non-finite.
    pub fn set_progress(&mut self, progress: Option<f32>) {
        if let Some(progress) = progress {
            assert!(progress.is_finite(), "spinner progress must be finite");
            self.progress = Some(progress.clamp(0.0, 1.0));
        } else {
            self.progress = None;
        }
    }

    /// Sets the normalized animation phase, wrapping it into `0.0..1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `phase` is non-finite.
    pub fn set_phase(&mut self, phase: f32) {
        assert!(phase.is_finite(), "spinner phase must be finite");
        self.phase = phase.rem_euclid(1.0);
    }

    /// Advances the animation phase by `delta`, wrapping it into `0.0..1.0`.
    ///
    /// # Panics
    ///
    /// Panics when `delta` is non-finite.
    pub fn advance_phase(&mut self, delta: f32) {
        assert!(delta.is_finite(), "spinner phase delta must be finite");
        self.set_phase(self.phase + delta);
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Spinner {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(constraints.constrain(size(self.diameter, self.diameter)))
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("spinner")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &DssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        let theme = environment.theme();
        let diameter = bounds.size.width().min(bounds.size.height()).max(0.0);
        if diameter <= 0.0 {
            return;
        }
        let thickness = (diameter * 0.12).max(1.5).min((diameter / 3.0).max(1.5));
        let ring_radius = ((diameter - thickness) / 2.0).max(0.0);
        let segment_length = (diameter * 0.28).max(thickness).min(ring_radius);
        if segment_length <= 0.0 {
            return;
        }
        let center = Point::new(
            bounds.origin.x + bounds.size.width() / 2.0,
            bounds.origin.y + bounds.size.height() / 2.0,
        );
        let segment_radius = (thickness / 2.0).min(segment_length / 2.0);
        let active_color = self
            .color
            .or(style.accent)
            .unwrap_or_else(|| theme.accent());
        let track_color = self
            .track_color
            .or(style.background)
            .unwrap_or_else(|| theme.control_background());
        let count = self.segments.max(MIN_SEGMENTS);
        let count_f32 = usize_to_f32(count);

        for index in 0..count {
            let angle = TAU * usize_to_f32(index) / count_f32;
            let transform = Affine::rotate(angle).then(Affine::translate(center.x, center.y));
            let segment = Rect::new(
                Point::new(-thickness / 2.0, -ring_radius),
                size(thickness, segment_length),
            );

            if let Some(progress) = self.progress {
                let coverage = (progress * count_f32 - usize_to_f32(index)).clamp(0.0, 1.0);
                let track = track_color.with_alpha(track_color.alpha * 0.35);
                context.with_transform(transform, |context| {
                    context.fill_rounded_rect(segment, segment_radius, track);
                });
                if coverage > 0.0 {
                    let color = active_color.with_alpha(active_color.alpha * coverage);
                    context.with_transform(transform, |context| {
                        context.fill_rounded_rect(segment, segment_radius, color);
                    });
                }
            } else {
                let offset = (usize_to_f32(index) / count_f32 - self.phase).rem_euclid(1.0);
                let intensity = 0.20 + 0.80 * (1.0 - offset).powi(2);
                let color = active_color.with_alpha(active_color.alpha * intensity);
                context.with_transform(transform, |context| {
                    context.fill_rounded_rect(segment, segment_radius, color);
                });
            }
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn accessibility(&self) -> Accessibility {
        let accessibility = Accessibility::new(AccessibilityRole::ProgressIndicator);
        match self.progress {
            Some(progress) => accessibility.with_numeric_value(f64::from(progress), 0.0, 1.0),
            None => accessibility.with_states(drawen_ui::AccessibilityState::BUSY),
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("spinner sizes are non-negative and not NaN")
}

fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use drawen_render::{DisplayCommand, DisplayList};
    use drawen_ui::{LightTheme, Theme, UiRuntime};

    use super::*;

    #[test]
    fn indeterminate_spinner_paints_one_segment_per_step() {
        let mut runtime = UiRuntime::new(Spinner::new().with_diameter(24.0).with_segments(5));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(24.0, 24.0)).expect("valid constraints"))
            .expect("spinner layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("spinner paint succeeds");

        let fills = display_list
            .commands()
            .iter()
            .filter(|command| matches!(command, DisplayCommand::FillRoundedRect { .. }))
            .count();
        assert_eq!(fills, 5);
    }

    #[test]
    fn segments_orbit_the_spinner_center() {
        let diameter = 24.0;
        let mut runtime = UiRuntime::new(Spinner::new().with_diameter(diameter).with_segments(8));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(diameter, diameter)).expect("valid constraints"))
            .expect("spinner layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("spinner paint succeeds");

        let center = Point::new(diameter / 2.0, diameter / 2.0);
        let mut transform = Affine::IDENTITY;
        let mut saved = Vec::new();
        let mut offsets = Vec::new();
        for command in display_list.commands() {
            match command {
                DisplayCommand::Save => saved.push(transform),
                DisplayCommand::Restore => {
                    transform = saved.pop().expect("spinner balances save and restore");
                }
                DisplayCommand::Transform { transform: next } => {
                    transform = transform.then(*next);
                }
                DisplayCommand::FillRoundedRect { rect, .. } => {
                    let segment_center = transform.transform_point(Point::new(
                        rect.origin.x + rect.size.width() / 2.0,
                        rect.origin.y + rect.size.height() / 2.0,
                    ));
                    let dx = f64::from(segment_center.x - center.x);
                    let dy = f64::from(segment_center.y - center.y);
                    offsets.push(dx.hypot(dy));
                }
                _ => {}
            }
        }

        assert_eq!(offsets.len(), 8);
        let expected = offsets[0];
        assert!(expected > 0.0 && expected < f64::from(diameter / 2.0));
        for offset in offsets {
            assert!(
                (offset - expected).abs() < 0.001,
                "every segment must sit on the same orbit, got {offset} and {expected}"
            );
        }
    }

    #[test]
    fn determinate_spinner_draws_a_track_and_the_completed_arc() {
        let mut runtime = UiRuntime::new(Spinner::with_progress(0.5).with_segments(8));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(40.0, 40.0)).expect("valid constraints"))
            .expect("spinner layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("spinner paint succeeds");

        let active = display_list
            .commands()
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DisplayCommand::FillRoundedRect { color, .. }
                        if *color == LightTheme.accent()
                )
            })
            .count();
        let fills = display_list
            .commands()
            .iter()
            .filter(|command| matches!(command, DisplayCommand::FillRoundedRect { .. }))
            .count();
        assert_eq!(active, 4);
        assert_eq!(fills, 8 + 4);
    }

    #[test]
    fn clamps_progress_and_wraps_the_phase() {
        let mut spinner = Spinner::with_progress(2.0);
        assert_eq!(spinner.progress(), Some(1.0));
        spinner.set_progress(Some(-0.5));
        assert_eq!(spinner.progress(), Some(0.0));
        spinner.set_phase(1.25);
        assert!((spinner.phase() - 0.25).abs() <= f32::EPSILON);
        spinner.advance_phase(0.5);
        assert!((spinner.phase() - 0.75).abs() <= f32::EPSILON);
    }
}
