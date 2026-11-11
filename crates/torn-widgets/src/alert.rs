use torn_core::{Color, Constraints, Insets, Point, Rect, Size};
use torn_render::{PaintContext, TextLayout, TextStyle};
use torn_ui::{
    Accessibility, AccessibilityRole, Border, FontWeight, LayoutContext, LayoutResult, StyleTarget,
    Theme, TssStyle, UiEnvironment, Widget,
};

const DEFAULT_PADDING: Insets = Insets::new(10.0, 14.0, 10.0, 16.0);
const DEFAULT_STRIPE_WIDTH: f32 = 3.0;

/// Semantic color family used by an [`Alert`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlertTone {
    /// Informational message using the theme accent.
    #[default]
    Info,
    /// Positive confirmation.
    Success,
    /// Caution or pending state.
    Warning,
    /// Error or destructive state.
    Danger,
}

impl AlertTone {
    fn color(self, theme: &dyn Theme) -> Color {
        match self {
            Self::Info => theme.accent(),
            Self::Success => theme.success(),
            Self::Warning => theme.warning(),
            Self::Danger => theme.danger(),
        }
    }
}

/// A tinted message banner with an optional title and body.
///
/// The alert owns its text so a single widget conveys a complete status
/// message. It is decorative: append it to a layout and use a surrounding
/// container for any actions.
pub struct Alert {
    tone: AlertTone,
    title: Option<String>,
    message: Option<String>,
    padding: Insets,
    corner_radius: Option<f32>,
    stripe_width: f32,
    title_layout: Option<TextLayout>,
    message_layout: Option<TextLayout>,
}

impl Alert {
    /// Creates an empty informational alert.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tone: AlertTone::Info,
            title: None,
            message: None,
            padding: DEFAULT_PADDING,
            corner_radius: None,
            stripe_width: DEFAULT_STRIPE_WIDTH,
            title_layout: None,
            message_layout: None,
        }
    }

    /// Returns the semantic tone.
    #[must_use]
    pub const fn tone(&self) -> AlertTone {
        self.tone
    }

    /// Returns the alert title, if one is configured.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the alert message, if one is configured.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Returns this alert with a bold title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Returns this alert with a body message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Returns this alert with a semantic tone.
    #[must_use]
    pub const fn with_tone(mut self, tone: AlertTone) -> Self {
        self.tone = tone;
        self
    }

    /// Returns this alert with explicit content padding.
    #[must_use]
    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    /// Returns this alert with an explicit corner radius.
    #[must_use]
    pub const fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius.max(0.0));
        self
    }

    /// Returns this alert with an explicit accent stripe width.
    #[must_use]
    pub const fn with_stripe_width(mut self, width: f32) -> Self {
        self.stripe_width = width.max(0.0);
        self
    }
}

impl Default for Alert {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Alert {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let theme = environment.theme();
        let padding = style.padding_or(self.padding);
        let max = constraints.max();
        let wrap_width = max
            .width()
            .is_finite()
            .then(|| (max.width() - padding.horizontal()).max(0.0));

        let title_color = style.foreground_or(theme.foreground());
        let message_color = style.foreground.map_or_else(
            || theme.muted_foreground(),
            |color| color.with_alpha(color.alpha * 0.75),
        );
        let base_font = environment.font_for(&style);

        let title_layout = self
            .title
            .as_deref()
            .filter(|title| !title.is_empty())
            .map(|title| {
                let mut font = base_font.clone();
                font.set_weight(FontWeight::BOLD);
                environment.text_shaper().layout(
                    title,
                    &TextStyle::new_with_font(font, title_color),
                    wrap_width,
                )
            });
        let message_layout = self
            .message
            .as_deref()
            .filter(|message| !message.is_empty())
            .map(|message| {
                environment.text_shaper().layout(
                    message,
                    &TextStyle::new_with_font(base_font, message_color),
                    wrap_width,
                )
            });

        let gap = if title_layout.is_some() && message_layout.is_some() {
            style.spacing_or(theme.spacing()) / 2.0
        } else {
            0.0
        };
        let content_width = title_layout
            .as_ref()
            .map_or(0.0, |layout| layout.size().width())
            .max(
                message_layout
                    .as_ref()
                    .map_or(0.0, |layout| layout.size().width()),
            );
        let content_height = title_layout
            .as_ref()
            .map_or(0.0, |layout| layout.size().height())
            + message_layout
                .as_ref()
                .map_or(0.0, |layout| layout.size().height())
            + gap;

        self.title_layout = title_layout;
        self.message_layout = message_layout;
        LayoutResult::new(constraints.constrain(size(
            content_width + padding.horizontal(),
            content_height + padding.vertical(),
        )))
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("alert")
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
        let tone = self.tone.color(theme);
        let radius = self
            .corner_radius
            .or(style.corner_radius)
            .unwrap_or_else(|| theme.corner_radius());
        let background = style
            .background
            .unwrap_or_else(|| blend(theme.raised_surface(), tone, 0.12));
        context.fill_rounded_rect(bounds, radius, background);
        let border = style
            .border
            .unwrap_or_else(|| Border::new(1.0, tone.with_alpha(tone.alpha * 0.55)));
        context.stroke_rounded_rect(bounds, radius, border.width, border.color);

        let padding = style.padding_or(self.padding);
        let stripe_height = (bounds.size.height() - 12.0).max(0.0);
        let stripe_width = self.stripe_width.min(bounds.size.width());
        if stripe_width > 0.0 && stripe_height > 0.0 {
            context.fill_rounded_rect(
                Rect::new(
                    Point::new(bounds.origin.x + 5.0, bounds.origin.y + 6.0),
                    size(stripe_width, stripe_height),
                ),
                stripe_width / 2.0,
                tone,
            );
        }

        let gap = if self.title_layout.is_some() && self.message_layout.is_some() {
            style.spacing_or(theme.spacing()) / 2.0
        } else {
            0.0
        };
        let mut text_y = bounds.origin.y + padding.top;
        if let Some(layout) = &self.title_layout {
            context.draw_text(
                layout.clone(),
                Point::new(bounds.origin.x + padding.left, text_y),
            );
            text_y += layout.size().height() + gap;
        }
        if let Some(layout) = &self.message_layout {
            context.draw_text(
                layout.clone(),
                Point::new(bounds.origin.x + padding.left, text_y),
            );
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::Group);
        if let Some(label) = self.title.as_ref().or(self.message.as_ref()) {
            accessibility = accessibility.with_label(label.clone());
        }
        accessibility
    }
}

fn blend(base: Color, overlay: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::rgba(
        base.red + (overlay.red - base.red) * amount,
        base.green + (overlay.green - base.green) * amount,
        base.blue + (overlay.blue - base.blue) * amount,
        base.alpha + (overlay.alpha - base.alpha) * amount,
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("alert sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::{LightTheme, Theme, UiRuntime};

    use super::*;

    #[test]
    fn lays_out_title_and_message_inside_the_content_box() {
        let alert = Alert::new()
            .with_title("Update available")
            .with_message("Restart the application to finish installing the update.");
        let mut runtime = UiRuntime::new(alert);
        let result = runtime
            .layout(Constraints::loose(size(240.0, 200.0)))
            .expect("alert layout succeeds");

        assert!(result.size().width() <= 240.0);
        assert!(result.size().height() > 20.0);
        assert!(result.size().height() < 200.0);
    }

    #[test]
    fn paints_the_tone_stripe_and_both_text_blocks() {
        let alert = Alert::new()
            .with_title("Delete file")
            .with_message("This action cannot be undone.")
            .with_tone(AlertTone::Danger);
        let mut runtime = UiRuntime::new(alert);
        let measured = runtime
            .layout(Constraints::loose(size(240.0, 200.0)))
            .expect("alert layout succeeds")
            .size();
        let mut display_list = DisplayList::new();
        runtime
            .layout(Constraints::tight(measured).expect("valid constraints"))
            .expect("alert relayout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("alert paint succeeds");

        assert!(display_list.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::FillRoundedRect { color, .. } if *color == LightTheme.danger()
        )));
        assert_eq!(
            display_list
                .commands()
                .iter()
                .filter(|command| matches!(command, DisplayCommand::DrawText { .. }))
                .count(),
            2
        );
        assert!(
            display_list
                .commands()
                .iter()
                .any(|command| matches!(command, DisplayCommand::StrokeRoundedRect { .. }))
        );
    }
}
