use torn_core::{Color, Insets};
use torn_render::{Font, FontStyle, FontWeight};

/// A small, typed set of visual values shared by standard widgets.
///
/// The trait intentionally exposes semantic values rather than a generic
/// key-value map. A future `.tss` stylesheet can resolve its cascade into a
/// `Theme` without changing widget APIs.
pub trait Theme {
    /// Main background color for application surfaces.
    fn background(&self) -> Color;

    /// Default foreground color for text and icons.
    fn foreground(&self) -> Color;

    /// Highlight color for focused and selected controls.
    fn accent(&self) -> Color;

    /// Foreground color for secondary labels and hints.
    ///
    /// The default derives a slightly transparent version of
    /// [`Self::foreground`] so existing themes remain usable without changes.
    fn muted_foreground(&self) -> Color {
        let foreground = self.foreground();
        Color::rgba(
            foreground.red,
            foreground.green,
            foreground.blue,
            foreground.alpha * 0.7,
        )
    }

    /// Color that communicates success or a positive state.
    fn success(&self) -> Color {
        Color::rgba8(46, 204, 113, 255)
    }

    /// Color that communicates a warning.
    fn warning(&self) -> Color {
        Color::rgba8(241, 196, 15, 255)
    }

    /// Color that communicates danger or a destructive action.
    fn danger(&self) -> Color {
        Color::rgba8(231, 76, 60, 255)
    }

    /// Background color for a custom title bar.
    fn title_bar_background(&self) -> Color {
        self.control_background()
    }

    /// Foreground color for a custom title bar.
    fn title_bar_foreground(&self) -> Color {
        self.foreground()
    }

    /// Hover background for a title-bar window control.
    fn title_bar_hover_background(&self) -> Color {
        self.control_hover_background()
    }

    /// Default background color for an interactive control surface.
    ///
    /// This is a neutral control surface, not "the button color": menu bars,
    /// toolbars, text fields, tracks, and checkboxes may all use it. TSS
    /// selectors such as `button { ... }` target individual nodes and do not
    /// rewrite this theme value.
    fn control_background(&self) -> Color {
        self.background()
    }

    /// Background color for a pressed control.
    ///
    /// The default keeps existing custom themes source-compatible. New themes
    /// should override it with an interaction-state color.
    fn control_pressed_background(&self) -> Color {
        self.accent()
    }

    /// Background color for a control whose pointer is hovering over it.
    ///
    /// The default preserves the appearance of themes written before hover
    /// styling was introduced.
    fn control_hover_background(&self) -> Color {
        self.control_background()
    }

    /// Default inset between a control edge and its content, in logical pixels.
    ///
    /// The default preserves the historic control padding for existing custom
    /// themes.
    fn control_padding(&self) -> Insets {
        Insets::all(8.0)
    }

    /// Standard gap between related controls, in logical pixels.
    fn spacing(&self) -> f32;

    /// Default text size, in logical pixels.
    fn font_size(&self) -> f32;

    /// Preferred font family for application text.
    ///
    /// Returning `None` lets the text shaper use its default face. Custom
    /// themes can return a registered family name to opt into a specific font.
    fn font_family(&self) -> Option<&str> {
        None
    }

    /// Default font weight for application text.
    fn font_weight(&self) -> FontWeight {
        FontWeight::NORMAL
    }

    /// Default font style for application text.
    fn font_style(&self) -> FontStyle {
        FontStyle::Normal
    }

    /// Returns the theme's default font request.
    ///
    /// The request combines [`Self::font_size`], [`Self::font_weight`],
    /// [`Self::font_style`], and [`Self::font_family`]. Widgets normally use
    /// [`crate::UiEnvironment::font_for`], which applies per-node TSS values on
    /// top of this request.
    fn font(&self) -> Font {
        let mut font = Font::from_pixel_size(self.font_size())
            .with_weight(self.font_weight())
            .with_style(self.font_style());
        if let Some(family) = self.font_family() {
            font.set_family(family);
        }
        font
    }

    /// Default corner radius for controls, in logical pixels.
    fn corner_radius(&self) -> f32;
}

/// Built-in dark color theme.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DarkTheme;

impl Theme for DarkTheme {
    fn background(&self) -> Color {
        Color::rgba8(30, 30, 30, 255)
    }

    fn foreground(&self) -> Color {
        Color::rgba8(230, 230, 230, 255)
    }

    fn accent(&self) -> Color {
        Color::rgba8(0, 122, 204, 255)
    }

    fn control_background(&self) -> Color {
        Color::rgba8(60, 60, 60, 255)
    }

    fn control_pressed_background(&self) -> Color {
        Color::rgba8(80, 80, 80, 255)
    }

    fn control_hover_background(&self) -> Color {
        Color::rgba8(70, 70, 70, 255)
    }

    fn muted_foreground(&self) -> Color {
        Color::rgba8(150, 150, 150, 255)
    }

    fn success(&self) -> Color {
        Color::rgba8(46, 204, 113, 255)
    }

    fn warning(&self) -> Color {
        Color::rgba8(241, 196, 15, 255)
    }

    fn danger(&self) -> Color {
        Color::rgba8(232, 17, 35, 255)
    }

    fn title_bar_background(&self) -> Color {
        Color::rgba8(24, 24, 24, 255)
    }

    fn title_bar_hover_background(&self) -> Color {
        Color::rgba8(45, 45, 45, 255)
    }

    fn spacing(&self) -> f32 {
        8.0
    }

    fn font_size(&self) -> f32 {
        14.0
    }

    fn corner_radius(&self) -> f32 {
        4.0
    }
}

/// Built-in light color theme.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightTheme;

impl Theme for LightTheme {
    fn background(&self) -> Color {
        Color::rgba8(250, 250, 250, 255)
    }

    fn foreground(&self) -> Color {
        Color::rgba8(30, 30, 30, 255)
    }

    fn accent(&self) -> Color {
        Color::rgba8(0, 103, 192, 255)
    }

    fn control_background(&self) -> Color {
        Color::rgba8(235, 235, 235, 255)
    }

    fn control_pressed_background(&self) -> Color {
        Color::rgba8(210, 210, 210, 255)
    }

    fn control_hover_background(&self) -> Color {
        Color::rgba8(225, 225, 225, 255)
    }

    fn muted_foreground(&self) -> Color {
        Color::rgba8(100, 100, 100, 255)
    }

    fn success(&self) -> Color {
        Color::rgba8(39, 174, 96, 255)
    }

    fn warning(&self) -> Color {
        Color::rgba8(230, 126, 34, 255)
    }

    fn danger(&self) -> Color {
        Color::rgba8(196, 43, 28, 255)
    }

    fn title_bar_background(&self) -> Color {
        Color::rgba8(240, 240, 240, 255)
    }

    fn title_bar_hover_background(&self) -> Color {
        Color::rgba8(220, 220, 220, 255)
    }

    fn spacing(&self) -> f32 {
        8.0
    }

    fn font_size(&self) -> f32 {
        14.0
    }

    fn corner_radius(&self) -> f32 {
        4.0
    }
}

/// Appearance reported by a platform adapter.
///
/// `torn-ui` has no platform dependency, so querying the operating system is
/// deliberately delegated to the future platform crate. The adapter creates a
/// [`SystemTheme`] from its observed appearance and updates it when the system
/// setting changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SystemAppearance {
    /// Use the light palette.
    Light,
    /// Use the dark palette.
    #[default]
    Dark,
}

/// Theme selected from the appearance supplied by the operating system.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SystemTheme {
    appearance: SystemAppearance,
}

impl SystemTheme {
    /// Creates a system theme using the appearance reported by a platform adapter.
    #[must_use]
    pub const fn new(appearance: SystemAppearance) -> Self {
        Self { appearance }
    }

    /// Returns the currently selected system appearance.
    #[must_use]
    pub const fn appearance(self) -> SystemAppearance {
        self.appearance
    }

    /// Updates this theme after the platform reports an appearance change.
    pub fn set_appearance(&mut self, appearance: SystemAppearance) {
        self.appearance = appearance;
    }
}

impl Theme for SystemTheme {
    fn background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.background(),
            SystemAppearance::Dark => DarkTheme.background(),
        }
    }

    fn foreground(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.foreground(),
            SystemAppearance::Dark => DarkTheme.foreground(),
        }
    }

    fn accent(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.accent(),
            SystemAppearance::Dark => DarkTheme.accent(),
        }
    }

    fn control_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.control_background(),
            SystemAppearance::Dark => DarkTheme.control_background(),
        }
    }

    fn control_pressed_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.control_pressed_background(),
            SystemAppearance::Dark => DarkTheme.control_pressed_background(),
        }
    }

    fn control_hover_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.control_hover_background(),
            SystemAppearance::Dark => DarkTheme.control_hover_background(),
        }
    }

    fn muted_foreground(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.muted_foreground(),
            SystemAppearance::Dark => DarkTheme.muted_foreground(),
        }
    }

    fn success(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.success(),
            SystemAppearance::Dark => DarkTheme.success(),
        }
    }

    fn warning(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.warning(),
            SystemAppearance::Dark => DarkTheme.warning(),
        }
    }

    fn danger(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.danger(),
            SystemAppearance::Dark => DarkTheme.danger(),
        }
    }

    fn title_bar_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.title_bar_background(),
            SystemAppearance::Dark => DarkTheme.title_bar_background(),
        }
    }

    fn title_bar_foreground(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.title_bar_foreground(),
            SystemAppearance::Dark => DarkTheme.title_bar_foreground(),
        }
    }

    fn title_bar_hover_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.title_bar_hover_background(),
            SystemAppearance::Dark => DarkTheme.title_bar_hover_background(),
        }
    }

    fn control_padding(&self) -> Insets {
        match self.appearance {
            SystemAppearance::Light => LightTheme.control_padding(),
            SystemAppearance::Dark => DarkTheme.control_padding(),
        }
    }

    fn spacing(&self) -> f32 {
        match self.appearance {
            SystemAppearance::Light => LightTheme.spacing(),
            SystemAppearance::Dark => DarkTheme.spacing(),
        }
    }

    fn font_size(&self) -> f32 {
        match self.appearance {
            SystemAppearance::Light => LightTheme.font_size(),
            SystemAppearance::Dark => DarkTheme.font_size(),
        }
    }

    fn corner_radius(&self) -> f32 {
        match self.appearance {
            SystemAppearance::Light => LightTheme.corner_radius(),
            SystemAppearance::Dark => DarkTheme.corner_radius(),
        }
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Insets};

    use super::{DarkTheme, LightTheme, SystemAppearance, SystemTheme, Theme};

    #[test]
    fn built_in_palettes_have_distinct_surfaces() {
        assert_ne!(DarkTheme.background(), LightTheme.background());
        assert_eq!(DarkTheme.foreground(), Color::rgba8(230, 230, 230, 255));
    }

    #[test]
    fn system_theme_tracks_the_reported_appearance() {
        let mut theme = SystemTheme::new(SystemAppearance::Light);
        assert_eq!(theme.background(), LightTheme.background());

        theme.set_appearance(SystemAppearance::Dark);
        assert_eq!(theme.appearance(), SystemAppearance::Dark);
        assert_eq!(theme.background(), DarkTheme.background());
    }

    #[test]
    fn default_button_values_preserve_the_legacy_geometry() {
        assert_eq!(LightTheme.control_padding(), Insets::all(8.0));
        assert_eq!(DarkTheme.control_padding(), Insets::all(8.0));
    }
}
