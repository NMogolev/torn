use std::{cell::RefCell, collections::HashMap};

use torn_render::{FontdueTextShaper, TextShaper};

use crate::{LightTheme, StyleSheet, StyleTarget, Theme, TssError, TssStyle};

/// Runtime-wide services and semantic settings available to widgets.
///
/// The environment is owned by [`crate::UiRuntime`], so a widget observes the
/// same values during both layout and paint. It intentionally uses logical
/// pixels: platform adapters convert physical input and output using
/// [`Self::scale_factor`].
pub struct UiEnvironment {
    theme: Box<dyn Theme>,
    stylesheet: Option<StyleSheet>,
    style_cache: RefCell<HashMap<StyleTarget, TssStyle>>,
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
            stylesheet: None,
            style_cache: RefCell::new(HashMap::new()),
            scale_factor: 1.0,
            text_shaper: Box::new(FontdueTextShaper::ubuntu_light()),
            locale: "und".to_owned(),
        }
    }

    /// Creates an environment by resolving TSS source over `theme`.
    ///
    /// TSS values override the supplied semantic theme; properties absent from
    /// the sheet preserve the fallback theme's behavior.
    ///
    /// # Errors
    ///
    /// Returns [`TssError`] if `source` is invalid.
    pub fn from_tss(theme: impl Theme + 'static, source: &str) -> Result<Self, TssError> {
        let stylesheet = StyleSheet::parse(source)?;
        let mut environment = Self::new(stylesheet.theme(theme));
        environment.stylesheet = Some(stylesheet);
        Ok(environment)
    }

    /// Returns the theme used by standard widgets.
    #[must_use]
    pub fn theme(&self) -> &dyn Theme {
        self.theme.as_ref()
    }

    /// Returns the parsed stylesheet, when the environment was created with
    /// TSS or a stylesheet was installed later.
    #[must_use]
    pub fn stylesheet(&self) -> Option<&StyleSheet> {
        self.stylesheet.as_ref()
    }

    /// Installs or removes the runtime stylesheet.
    ///
    /// Widgets resolve their node styles from this sheet during the style pass.
    /// The semantic theme remains the fallback for values the sheet omits.
    pub fn set_stylesheet(&mut self, stylesheet: Option<StyleSheet>) {
        self.stylesheet = stylesheet;
        self.style_cache.get_mut().clear();
    }

    /// Resolves the cascade for `target`.
    ///
    /// Results are cached because the runtime asks for the same control style
    /// repeatedly while laying out and painting. Custom widgets and tooling can
    /// use this method to resolve styles with exactly the same contract as
    /// standard widgets.
    #[must_use]
    pub fn style_for(&self, target: &StyleTarget) -> TssStyle {
        if let Some(style) = self.style_cache.borrow().get(target) {
            return *style;
        }
        let style = self
            .stylesheet
            .as_ref()
            .map_or_else(TssStyle::default, |stylesheet| stylesheet.resolve(target));
        self.style_cache.borrow_mut().insert(target.clone(), style);
        style
    }

    /// Resolves the `:root` style used as the inheritance root.
    #[must_use]
    pub fn root_style(&self) -> TssStyle {
        self.style_for(&StyleTarget::root())
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

#[cfg(test)]
mod tests {
    use torn_core::Color;

    use super::UiEnvironment;
    use crate::{LightTheme, StyleTarget, Theme};

    #[test]
    fn constructs_a_theme_from_tss_source() {
        let environment = UiEnvironment::from_tss(
            LightTheme,
            ":root { color: #112233; } button { background: #445566; }",
        )
        .expect("valid TSS");

        assert_eq!(
            environment.theme().foreground(),
            Color::rgba8(17, 34, 51, 255)
        );
        assert_eq!(
            environment.theme().control_background(),
            LightTheme.control_background()
        );
        assert_eq!(
            environment
                .style_for(&StyleTarget::new("button"))
                .background,
            Some(Color::rgba8(68, 85, 102, 255))
        );
    }
}
