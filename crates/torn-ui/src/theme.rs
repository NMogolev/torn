use torn_core::Color;

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
    use torn_core::Color;

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
}
