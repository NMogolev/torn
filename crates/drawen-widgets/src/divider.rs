use drawen_core::{Color, Constraints, Rect, Size};
use drawen_render::PaintContext;
use drawen_ui::{DssStyle, LayoutContext, LayoutResult, StyleTarget, UiEnvironment, Widget};

const DEFAULT_LENGTH: f32 = 120.0;

/// Orientation of a [`Divider`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DividerAxis {
    /// A horizontal line that separates content stacked vertically.
    #[default]
    Horizontal,
    /// A vertical line that separates content placed side by side.
    Vertical,
}

/// A thin rule that separates groups of content.
///
/// The divider fills the available space along its main axis and uses its
/// thickness on the cross axis. It is decorative and never receives pointer
/// input.
pub struct Divider {
    axis: DividerAxis,
    thickness: f32,
    color: Option<Color>,
}

impl Divider {
    /// Creates a horizontal divider with a one-pixel rule.
    #[must_use]
    pub fn new() -> Self {
        Self {
            axis: DividerAxis::Horizontal,
            thickness: 1.0,
            color: None,
        }
    }

    /// Creates a vertical divider with a one-pixel rule.
    #[must_use]
    pub fn vertical() -> Self {
        Self {
            axis: DividerAxis::Vertical,
            thickness: 1.0,
            color: None,
        }
    }

    /// Returns the divider orientation.
    #[must_use]
    pub const fn axis(&self) -> DividerAxis {
        self.axis
    }

    /// Returns the rule thickness in logical pixels.
    #[must_use]
    pub const fn thickness(&self) -> f32 {
        self.thickness
    }

    /// Returns this divider with an explicit orientation.
    #[must_use]
    pub const fn with_axis(mut self, axis: DividerAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Returns this divider with an explicit rule thickness.
    #[must_use]
    pub fn with_thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness.max(0.0);
        self
    }

    /// Returns this divider with an explicit rule color.
    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Divider {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        let result = match self.axis {
            DividerAxis::Horizontal => size(
                if max.width().is_finite() {
                    max.width()
                } else {
                    DEFAULT_LENGTH
                },
                self.thickness,
            ),
            DividerAxis::Vertical => size(
                self.thickness,
                if max.height().is_finite() {
                    max.height()
                } else {
                    DEFAULT_LENGTH
                },
            ),
        };
        LayoutResult::new(constraints.constrain(result))
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("divider")
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
        let color = self
            .color
            .or(style.background)
            .or(style.foreground)
            .unwrap_or_else(|| environment.theme().border());
        context.fill_rect(bounds, color);
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("divider sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use drawen_render::{DisplayCommand, DisplayList};
    use drawen_ui::UiRuntime;

    use super::*;

    #[test]
    fn fills_the_main_axis_and_uses_its_thickness() {
        let mut runtime = UiRuntime::new(Divider::new().with_thickness(2.0));
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(80.0, 2.0)).expect("valid constraints"))
            .expect("divider layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("divider paint succeeds");

        assert!(matches!(
            display_list.commands(),
            [DisplayCommand::FillRect { rect, .. }]
                if (rect.size.width() - 80.0).abs() <= f32::EPSILON
                    && (rect.size.height() - 2.0).abs() <= f32::EPSILON
        ));
    }
}
