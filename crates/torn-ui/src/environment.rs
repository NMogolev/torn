use torn_render::{FontdueTextShaper, TextShaper};

use crate::{LightTheme, Theme};

/// Runtime-wide services and semantic settings available to widgets.
///
/// The environment is owned by [`crate::UiRuntime`], so a widget observes the
/// same values during both layout and paint. It intentionally uses logical
/// pixels: platform adapters convert physical input and output using
/// [`Self::scale_factor`].
pub struct UiEnvironment {
    theme: Box<dyn Theme>,
    scale_factor: f32,
    text_shaper: Box<dyn TextShaper>,
    locale: String,
}

impl UiEnvironment {
    /// Creates an environment with `theme`, a scale factor of `1.0`, Torn's
    /// bundled text services, and the invariant locale.
    #[must_use]
    pub fn new(theme: impl Theme + 'static) -> Self {
        Self {
            theme: Box::new(theme),
            scale_factor: 1.0,
            text_shaper: Box::new(FontdueTextShaper::ubuntu_light()),
            locale: "und".to_owned(),
        }
    }

    /// Returns the theme used by standard widgets.
    #[must_use]
    pub fn theme(&self) -> &dyn Theme {
        self.theme.as_ref()
    }

    /// Replaces the theme used by standard widgets.
    pub fn set_theme(&mut self, theme: impl Theme + 'static) {
        self.theme = Box::new(theme);
    }

    /// Returns the number of physical pixels per logical pixel.
    #[must_use]
    pub const fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Updates the number of physical pixels per logical pixel.
    ///
    /// # Panics
    ///
    /// Panics when `scale_factor` is not finite or is not positive.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        assert!(
            scale_factor.is_finite() && scale_factor > 0.0,
            "scale factor must be finite and positive"
        );
        self.scale_factor = scale_factor;
    }

    /// Returns the text shaper used by standard widgets.
    #[must_use]
    pub fn text_shaper(&self) -> &dyn TextShaper {
        self.text_shaper.as_ref()
    }

    /// Supplies the text shaper used by widgets that shape text at runtime.
    pub fn set_text_shaper(&mut self, text_shaper: impl TextShaper + 'static) {
        self.text_shaper = Box::new(text_shaper);
    }

    /// Restores Torn's bundled default text shaper.
    pub fn clear_text_shaper(&mut self) {
        self.text_shaper = Box::new(FontdueTextShaper::ubuntu_light());
    }

    /// Returns the locale requested by the application.
    #[must_use]
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Updates the locale requested by the application.
    pub fn set_locale(&mut self, locale: impl Into<String>) {
        self.locale = locale.into();
    }
}

impl Default for UiEnvironment {
    fn default() -> Self {
        Self::new(LightTheme)
    }
}
