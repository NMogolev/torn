use std::time::Duration;

use drawen_core::{Color, Insets};
use drawen_render::{Font, FontStyle, FontWeight};

/// A small, typed set of visual values shared by standard widgets.
///
/// The trait intentionally exposes semantic values rather than a generic
/// key-value map. A future `.dss` stylesheet can resolve its cascade into a
/// `Theme` without changing widget APIs.
pub trait Theme {
    /// Main background color for application surfaces.
    fn background(&self) -> Color;

    /// Default foreground color for text and icons.
    fn foreground(&self) -> Color;

    /// Highlight color for focused and selected controls.
    fn accent(&self) -> Color;

    /// Main surface color for panels, cards, popups, and input fields.
    ///
    /// The default preserves the single-surface appearance of older custom
    /// themes. Modern themes usually return a slightly different tone from
    /// [`Self::background`].
    fn surface(&self) -> Color {
        self.background()
    }

    /// Surface color raised above [`Self::surface`], such as a card or menu.
    fn raised_surface(&self) -> Color {
        self.control_background()
    }

    /// Default outline color for dividers, inputs, and grouped surfaces.
    ///
    /// The default is a translucent foreground tint, so it follows both light
    /// and dark custom palettes without requiring another theme value.
    fn border(&self) -> Color {
        let foreground = self.foreground();
        Color::rgba(
            foreground.red,
            foreground.green,
            foreground.blue,
            foreground.alpha * 0.16,
        )
    }

    /// Color of the keyboard focus ring painted around the focused control.
    fn focus_ring(&self) -> Color {
        self.accent()
    }

    /// Tint used for a soft drop shadow below elevated surfaces.
    fn shadow(&self) -> Color {
        Color::rgba(0.0, 0.0, 0.0, 0.45)
    }

    /// Text and icon color placed on top of an [`Self::accent`] fill.
    fn accent_foreground(&self) -> Color {
        Color::WHITE
    }

    /// Background tint behind selected text or rows.
    fn selection(&self) -> Color {
        self.accent().with_alpha(0.24)
    }

    /// Foreground color for disabled text and icons.
    fn disabled_foreground(&self) -> Color {
        self.muted_foreground()
    }

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
    /// toolbars, text fields, tracks, and checkboxes may all use it. DSS
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
    /// [`crate::UiEnvironment::font_for`], which applies per-node DSS values on
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

    /// Duration of hover, press, and focus color transitions.
    ///
    /// Return [`Duration::ZERO`] to switch interactions instantly for this
    /// theme. Applications can also disable animation globally through
    /// [`crate::UiEnvironment::set_animations_enabled`].
    fn animation_duration(&self) -> Duration {
        Duration::from_millis(140)
    }
}

/// Built-in dark color theme.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DarkTheme;

impl Theme for DarkTheme {
    fn background(&self) -> Color {
        Color::rgba8(15, 17, 23, 255)
    }

    fn surface(&self) -> Color {
        Color::rgba8(22, 26, 35, 255)
    }

    fn raised_surface(&self) -> Color {
        Color::rgba8(28, 34, 48, 255)
    }

    fn foreground(&self) -> Color {
        Color::rgba8(231, 235, 243, 255)
    }

    fn accent(&self) -> Color {
        Color::rgba8(124, 156, 255, 255)
    }

    fn accent_foreground(&self) -> Color {
        Color::rgba8(11, 16, 32, 255)
    }

    fn control_background(&self) -> Color {
        Color::rgba8(34, 42, 57, 255)
    }

    fn control_pressed_background(&self) -> Color {
        Color::rgba8(55, 66, 90, 255)
    }

    fn control_hover_background(&self) -> Color {
        Color::rgba8(43, 52, 70, 255)
    }

    fn border(&self) -> Color {
        Color::rgba8(44, 53, 72, 255)
    }

    fn focus_ring(&self) -> Color {
        Color::rgba8(143, 180, 255, 255)
    }

    fn shadow(&self) -> Color {
        Color::rgba8(0, 0, 0, 150)
    }

    fn selection(&self) -> Color {
        Color::rgba8(124, 156, 255, 64)
    }

    fn muted_foreground(&self) -> Color {
        Color::rgba8(154, 166, 188, 255)
    }

    fn disabled_foreground(&self) -> Color {
        Color::rgba8(110, 121, 140, 255)
    }

    fn success(&self) -> Color {
        Color::rgba8(74, 222, 128, 255)
    }

    fn warning(&self) -> Color {
        Color::rgba8(251, 191, 36, 255)
    }

    fn danger(&self) -> Color {
        Color::rgba8(251, 113, 133, 255)
    }

    fn title_bar_background(&self) -> Color {
        Color::rgba8(17, 21, 29, 255)
    }

    fn title_bar_hover_background(&self) -> Color {
        Color::rgba8(32, 40, 56, 255)
    }

    fn control_padding(&self) -> Insets {
        Insets::new(7.0, 12.0, 7.0, 12.0)
    }

    fn spacing(&self) -> f32 {
        8.0
    }

    fn font_size(&self) -> f32 {
        14.0
    }

    fn corner_radius(&self) -> f32 {
        8.0
    }
}

/// Built-in light color theme.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightTheme;

impl Theme for LightTheme {
    fn background(&self) -> Color {
        Color::rgba8(246, 247, 251, 255)
    }

    fn surface(&self) -> Color {
        Color::rgba8(255, 255, 255, 255)
    }

    fn raised_surface(&self) -> Color {
        Color::rgba8(255, 255, 255, 255)
    }

    fn foreground(&self) -> Color {
        Color::rgba8(27, 31, 39, 255)
    }

    fn accent(&self) -> Color {
        Color::rgba8(37, 99, 235, 255)
    }

    fn accent_foreground(&self) -> Color {
        Color::WHITE
    }

    fn control_background(&self) -> Color {
        Color::rgba8(238, 241, 246, 255)
    }

    fn control_pressed_background(&self) -> Color {
        Color::rgba8(216, 223, 234, 255)
    }

    fn control_hover_background(&self) -> Color {
        Color::rgba8(228, 233, 242, 255)
    }

    fn border(&self) -> Color {
        Color::rgba8(213, 219, 230, 255)
    }

    fn focus_ring(&self) -> Color {
        Color::rgba8(59, 130, 246, 255)
    }

    fn shadow(&self) -> Color {
        Color::rgba8(22, 30, 50, 56)
    }

    fn selection(&self) -> Color {
        Color::rgba8(37, 99, 235, 64)
    }

    fn muted_foreground(&self) -> Color {
        Color::rgba8(98, 107, 122, 255)
    }

    fn disabled_foreground(&self) -> Color {
        Color::rgba8(150, 158, 170, 255)
    }

    fn success(&self) -> Color {
        Color::rgba8(22, 163, 74, 255)
    }

    fn warning(&self) -> Color {
        Color::rgba8(217, 119, 6, 255)
    }

    fn danger(&self) -> Color {
        Color::rgba8(220, 38, 38, 255)
    }

    fn title_bar_background(&self) -> Color {
        Color::rgba8(241, 243, 248, 255)
    }

    fn title_bar_hover_background(&self) -> Color {
        Color::rgba8(227, 232, 241, 255)
    }

    fn control_padding(&self) -> Insets {
        Insets::new(7.0, 12.0, 7.0, 12.0)
    }

    fn spacing(&self) -> f32 {
        8.0
    }

    fn font_size(&self) -> f32 {
        14.0
    }

    fn corner_radius(&self) -> f32 {
        8.0
    }
}

/// Appearance reported by a platform adapter.
///
/// `drawen-ui` has no platform dependency, so querying the operating system is
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

    fn surface(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.surface(),
            SystemAppearance::Dark => DarkTheme.surface(),
        }
    }

    fn raised_surface(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.raised_surface(),
            SystemAppearance::Dark => DarkTheme.raised_surface(),
        }
    }

    fn border(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.border(),
            SystemAppearance::Dark => DarkTheme.border(),
        }
    }

    fn focus_ring(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.focus_ring(),
            SystemAppearance::Dark => DarkTheme.focus_ring(),
        }
    }

    fn shadow(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.shadow(),
            SystemAppearance::Dark => DarkTheme.shadow(),
        }
    }

    fn accent_foreground(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.accent_foreground(),
            SystemAppearance::Dark => DarkTheme.accent_foreground(),
        }
    }

    fn selection(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.selection(),
            SystemAppearance::Dark => DarkTheme.selection(),
        }
    }

    fn disabled_foreground(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.disabled_foreground(),
            SystemAppearance::Dark => DarkTheme.disabled_foreground(),
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
    use drawen_core::{Color, Insets};

    use super::{DarkTheme, LightTheme, SystemAppearance, SystemTheme, Theme};

    #[test]
    fn built_in_palettes_have_distinct_surfaces() {
        assert_ne!(DarkTheme.background(), LightTheme.background());
        assert_ne!(DarkTheme.surface(), DarkTheme.background());
        assert_ne!(LightTheme.surface(), LightTheme.background());
        assert_eq!(DarkTheme.foreground(), Color::rgba8(231, 235, 243, 255));
        assert_eq!(DarkTheme.accent_foreground(), Color::rgba8(11, 16, 32, 255));
        assert!(LightTheme.shadow().alpha < DarkTheme.shadow().alpha);
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
    fn built_in_controls_use_comfortable_padding_and_corners() {
        assert_eq!(
            LightTheme.control_padding(),
            Insets::new(7.0, 12.0, 7.0, 12.0)
        );
        assert_eq!(
            DarkTheme.control_padding(),
            Insets::new(7.0, 12.0, 7.0, 12.0)
        );
        assert!((DarkTheme.corner_radius() - 8.0).abs() < f32::EPSILON);
    }
}
