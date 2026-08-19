use torn_core::{Color, Insets};

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

    /// Default background color for a button or similarly raised control.
    ///
    /// The default keeps existing custom themes source-compatible. New themes
    /// should override it instead of relying on an application surface color.
    fn button_background(&self) -> Color {
        self.background()
    }

    /// Background color for a pressed button or similarly active control.
    ///
    /// The default keeps existing custom themes source-compatible. New themes
    /// should override it with an interaction-state color.
    fn button_pressed_background(&self) -> Color {
        self.accent()
    }

    /// Background color for a button whose pointer is hovering over it.
    ///
    /// The default preserves the appearance of themes written before hover
    /// styling was introduced.
    fn button_hover_background(&self) -> Color {
        self.button_background()
    }

    /// Default inset between a button edge and its child, in logical pixels.
    ///
    /// The default preserves the historic button padding for existing custom
    /// themes.
    fn button_padding(&self) -> Insets {
        Insets::all(8.0)
    }

    /// Standard gap between related controls, in logical pixels.
    fn spacing(&self) -> f32;

    /// Default text size, in logical pixels.
    fn font_size(&self) -> f32;

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

    fn button_background(&self) -> Color {
        Color::rgba8(60, 60, 60, 255)
    }

    fn button_pressed_background(&self) -> Color {
        Color::rgba8(80, 80, 80, 255)
    }

    fn button_hover_background(&self) -> Color {
        Color::rgba8(70, 70, 70, 255)
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

    fn button_background(&self) -> Color {
        Color::rgba8(235, 235, 235, 255)
    }

    fn button_pressed_background(&self) -> Color {
        Color::rgba8(210, 210, 210, 255)
    }

    fn button_hover_background(&self) -> Color {
        Color::rgba8(225, 225, 225, 255)
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

    fn button_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.button_background(),
            SystemAppearance::Dark => DarkTheme.button_background(),
        }
    }

    fn button_pressed_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.button_pressed_background(),
            SystemAppearance::Dark => DarkTheme.button_pressed_background(),
        }
    }

    fn button_hover_background(&self) -> Color {
        match self.appearance {
            SystemAppearance::Light => LightTheme.button_hover_background(),
            SystemAppearance::Dark => DarkTheme.button_hover_background(),
        }
    }

    fn button_padding(&self) -> Insets {
        match self.appearance {
            SystemAppearance::Light => LightTheme.button_padding(),
            SystemAppearance::Dark => DarkTheme.button_padding(),
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
        assert_eq!(LightTheme.button_padding(), Insets::all(8.0));
        assert_eq!(DarkTheme.button_padding(), Insets::all(8.0));
    }
}
