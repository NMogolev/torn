use torn_core::{Color, Constraints, Point, Rect, Size};
use torn_render::{PaintContext, TextLayout, TextStyle};
use torn_ui::{
    Accessibility, AccessibilityRole, ChildLayout, LayoutContext, LayoutResult, StyleTarget, Theme,
    TssStyle, UiEnvironment, Widget,
};

const DEFAULT_DIAMETER: f32 = 40.0;

/// Presence indicator displayed on an [`Avatar`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvatarStatus {
    /// The user is available.
    Online,
    /// The user is idle or away.
    Away,
    /// The user does not want to be disturbed.
    Busy,
    /// The user is offline.
    Offline,
}

impl AvatarStatus {
    fn color(self, theme: &dyn Theme) -> Color {
        match self {
            Self::Online => theme.success(),
            Self::Away => theme.warning(),
            Self::Busy => theme.danger(),
            Self::Offline => theme.muted_foreground(),
        }
    }
}

/// A circular identity surface for initials or a runtime-owned child.
///
/// The avatar fills its bounds with a circle, centers optional initials, and
/// paints an optional presence dot in the bottom-right corner. Append a single
/// child when an image or custom content should replace the initials.
pub struct Avatar {
    diameter: f32,
    initials: Option<String>,
    label: Option<String>,
    background: Option<Color>,
    foreground: Option<Color>,
    status: Option<AvatarStatus>,
    initials_layout: Option<TextLayout>,
}

impl Avatar {
    /// Creates an empty avatar with the default diameter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            diameter: DEFAULT_DIAMETER,
            initials: None,
            label: None,
            background: None,
            foreground: None,
            status: None,
            initials_layout: None,
        }
    }

    /// Returns the requested diameter in logical pixels.
    #[must_use]
    pub const fn diameter(&self) -> f32 {
        self.diameter
    }

    /// Returns the initials painted by this avatar, if any.
    #[must_use]
    pub fn initials(&self) -> Option<&str> {
        self.initials.as_deref()
    }

    /// Returns the presence indicator, if one is configured.
    #[must_use]
    pub const fn status(&self) -> Option<AvatarStatus> {
        self.status
    }

    /// Returns this avatar with an explicit diameter.
    #[must_use]
    pub fn with_diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(0.0);
        self
    }

    /// Returns this avatar with initials instead of custom child content.
    #[must_use]
    pub fn with_initials(mut self, initials: impl Into<String>) -> Self {
        self.initials = Some(initials.into());
        self
    }

    /// Returns this avatar with an accessible name.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Returns this avatar with an explicit circle color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns this avatar with an explicit initials color.
    #[must_use]
    pub const fn with_foreground(mut self, color: Color) -> Self {
        self.foreground = Some(color);
        self
    }

    /// Returns this avatar with a presence indicator.
    #[must_use]
    pub const fn with_status(mut self, status: AvatarStatus) -> Self {
        self.status = Some(status);
        self
    }
}

impl Default for Avatar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Avatar {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let target = constraints.constrain(size(self.diameter, self.diameter));
        let diameter = target.width().min(target.height());
        let circle_origin = Point::new(
            (target.width() - diameter) / 2.0,
            (target.height() - diameter) / 2.0,
        );

        let mut children = Vec::new();
        let has_child = context.child_count() == 1;
        if has_child {
            let inset = diameter * 0.14;
            let inner = size(
                (diameter - inset * 2.0).max(0.0),
                (diameter - inset * 2.0).max(0.0),
            );
            let (id, layout) = context
                .layout_child(0, Constraints::loose(inner))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(
                id,
                Point::new(
                    circle_origin.x + (diameter - layout.size().width()) / 2.0,
                    circle_origin.y + (diameter - layout.size().height()) / 2.0,
                ),
            ));
        }

        let environment = context.environment();
        self.initials_layout = if has_child {
            None
        } else {
            self.initials.as_ref().map(|initials| {
                let mut font = environment.font_for(&style);
                if style.font_size.is_none() {
                    font.set_pixel_size(diameter * 0.40);
                }
                let color = self
                    .foreground
                    .or(style.foreground)
                    .unwrap_or_else(|| environment.theme().foreground());
                environment.text_shaper().layout(
                    initials,
                    &TextStyle::new_with_font(font, color),
                    None,
                )
            })
        };
        LayoutResult::with_children(target, children)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("avatar")
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
        let diameter = bounds.size.width().min(bounds.size.height()).max(0.0);
        if diameter <= 0.0 {
            return;
        }
        let theme = environment.theme();
        let circle = Rect::new(
            Point::new(
                bounds.origin.x + (bounds.size.width() - diameter) / 2.0,
                bounds.origin.y + (bounds.size.height() - diameter) / 2.0,
            ),
            size(diameter, diameter),
        );
        let background = self
            .background
            .or(style.background)
            .unwrap_or_else(|| theme.control_background());
        context.fill_rounded_rect(circle, diameter / 2.0, background);
        if let Some(border) = style.border {
            context.stroke_rounded_rect(circle, diameter / 2.0, border.width, border.color);
        }

        if let Some(layout) = &self.initials_layout {
            let text_size = layout.size();
            context.draw_text(
                layout.clone(),
                Point::new(
                    circle.origin.x + (diameter - text_size.width()) / 2.0,
                    circle.origin.y + (diameter - text_size.height()) / 2.0,
                ),
            );
        }

        if let Some(status) = self.status {
            let dot = (diameter * 0.28).max(4.0).min(diameter);
            let gap = (diameter * 0.05).max(1.0);
            let offset = (diameter / 2.0 - dot / 2.0 - gap) * std::f32::consts::FRAC_1_SQRT_2;
            let dot_rect = Rect::new(
                Point::new(
                    circle.origin.x + diameter / 2.0 + offset - dot / 2.0,
                    circle.origin.y + diameter / 2.0 + offset - dot / 2.0,
                ),
                size(dot, dot),
            );
            let ring = Rect::new(
                Point::new(dot_rect.origin.x - gap, dot_rect.origin.y - gap),
                size(dot + gap * 2.0, dot + gap * 2.0),
            );
            context.fill_rounded_rect(ring, ring.size.width() / 2.0, background);
            context.fill_rounded_rect(dot_rect, dot / 2.0, status.color(theme));
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::Image);
        if let Some(label) = self.label.as_ref().or(self.initials.as_ref()) {
            accessibility = accessibility.with_label(label.clone());
        }
        accessibility
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("avatar sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn centers_initials_inside_a_circle_and_paints_a_status_dot() {
        let avatar = Avatar::new()
            .with_diameter(48.0)
            .with_initials("AB")
            .with_status(AvatarStatus::Online);
        let mut runtime = UiRuntime::new(avatar);
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(size(48.0, 48.0)).expect("valid constraints"))
            .expect("avatar layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("avatar paint succeeds");

        assert!(matches!(
            display_list.commands().first(),
            Some(DisplayCommand::FillRoundedRect { radius, .. })
                if (*radius - 24.0).abs() <= f32::EPSILON
        ));
        assert!(
            display_list
                .commands()
                .iter()
                .any(|command| matches!(command, DisplayCommand::DrawText { .. }))
        );
        assert_eq!(
            display_list
                .commands()
                .iter()
                .filter(|command| matches!(command, DisplayCommand::FillRoundedRect { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn constrains_the_requested_diameter_to_the_available_space() {
        let mut runtime = UiRuntime::new(Avatar::new().with_diameter(64.0));
        let result = runtime
            .layout(Constraints::tight(size(32.0, 32.0)).expect("valid constraints"))
            .expect("avatar layout succeeds");
        assert_eq!(result.size(), size(32.0, 32.0));
    }
}
