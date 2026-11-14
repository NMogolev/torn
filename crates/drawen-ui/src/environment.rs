use std::{cell::RefCell, collections::HashMap};

use drawen_render::{DEFAULT_DPI, Font, FontdueTextShaper, TextShaper};

use crate::{DssError, DssStyle, LightTheme, StyleSheet, StyleTarget, Theme};

/// Runtime-wide services and semantic settings available to widgets.
///
/// The environment is owned by [`crate::UiRuntime`], so a widget observes the
/// same values during both layout and paint. It intentionally uses logical
/// pixels: platform adapters convert physical input and output using
/// [`Self::scale_factor`].
pub struct UiEnvironment {
    theme: Box<dyn Theme>,
    stylesheet: Option<StyleSheet>,
    style_cache: RefCell<HashMap<StyleTarget, DssStyle>>,
    scale_factor: f32,
    logical_dpi: f32,
    application_font: Option<Font>,
    text_shaper: Box<dyn TextShaper>,
    text_shaper_generation: u64,
    locale: String,
    animations_enabled: bool,
}

impl UiEnvironment {
    /// Creates an environment with `theme`, a scale factor of `1.0`, Drawen's
    /// bundled text services, and the invariant locale.
    #[must_use]
    pub fn new(theme: impl Theme + 'static) -> Self {
        Self {
            theme: Box::new(theme),
            stylesheet: None,
            style_cache: RefCell::new(HashMap::new()),
            scale_factor: 1.0,
            logical_dpi: DEFAULT_DPI,
            application_font: None,
            text_shaper: Box::new(FontdueTextShaper::ubuntu_light()),
            text_shaper_generation: 0,
            locale: "und".to_owned(),
            animations_enabled: true,
        }
    }

    /// Creates an environment by resolving DSS source over `theme`.
    ///
    /// DSS values override the supplied semantic theme; properties absent from
    /// the sheet preserve the fallback theme's behavior.
    ///
    /// # Errors
    ///
    /// Returns [`DssError`] if `source` is invalid.
    pub fn from_dss(theme: impl Theme + 'static, source: &str) -> Result<Self, DssError> {
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
    /// DSS or a stylesheet was installed later.
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
    pub fn style_for(&self, target: &StyleTarget) -> DssStyle {
        if let Some(style) = self.style_cache.borrow().get(target) {
            return style.clone();
        }
        let style = self
            .stylesheet
            .as_ref()
            .map_or_else(DssStyle::default, |stylesheet| stylesheet.resolve(target));
        self.style_cache
            .borrow_mut()
            .insert(target.clone(), style.clone());
        style
    }

    /// Resolves the `:root` style used as the inheritance root.
    #[must_use]
    pub fn root_style(&self) -> DssStyle {
        self.style_for(&StyleTarget::root())
    }

    /// Replaces the theme used by standard widgets.
    pub fn set_theme(&mut self, theme: impl Theme + 'static) {
        self.theme = Box::new(theme);
    }

    /// Returns the application-wide default font request.
    ///
    /// An explicit application font takes precedence; otherwise the request is
    /// derived from the current theme. This is the application-font equivalent
    /// of `QApplication::font()`.
    #[must_use]
    pub fn default_font(&self) -> Font {
        self.application_font
            .clone()
            .unwrap_or_else(|| self.theme.font())
    }

    /// Sets an application-wide default font that overrides the theme font.
    pub fn set_default_font(&mut self, font: Font) {
        self.application_font = Some(font);
    }

    /// Restores the theme-derived default font.
    pub fn clear_default_font(&mut self) {
        self.application_font = None;
    }

    /// Resolves a node style into a font request.
    ///
    /// The result starts from [`Self::default_font`] and applies the style's
    /// `font-family`, `font-size`, `font-weight`, and `font-style` values.
    #[must_use]
    pub fn font_for(&self, style: &DssStyle) -> Font {
        self.font_from(&self.default_font(), style)
    }

    /// Applies style font values on top of an existing font request.
    ///
    /// This is useful for widgets that carry their own base font, such as
    /// [`crate::UiEnvironment`]-independent previews or document text.
    #[must_use]
    pub fn font_from(&self, base: &Font, style: &DssStyle) -> Font {
        let mut font = base.clone();
        if let Some(families) = &style.font_families {
            font.set_families(families.iter().cloned());
        }
        if let Some(size) = style.font_size {
            font.set_pixel_size(size.pixels_at(self.logical_dpi));
        }
        if let Some(weight) = style.font_weight {
            font.set_weight(weight);
        }
        if let Some(font_style) = style.font_style {
            font.set_style(font_style);
        }
        font
    }

    /// Resolves a node style's font size to logical pixels.
    #[must_use]
    pub fn font_size_for(&self, style: &DssStyle) -> f32 {
        style
            .font_size_pixels(self.logical_dpi)
            .unwrap_or_else(|| self.default_font().pixel_size(self.logical_dpi))
    }

    /// Returns the logical DPI used to resolve typographic point sizes.
    #[must_use]
    pub const fn logical_dpi(&self) -> f32 {
        self.logical_dpi
    }

    /// Updates the logical DPI used to resolve typographic point sizes.
    ///
    /// # Panics
    ///
    /// Panics when `dpi` is not finite or is not positive.
    pub fn set_logical_dpi(&mut self, dpi: f32) {
        assert!(
            dpi.is_finite() && dpi > 0.0,
            "logical DPI must be finite and positive"
        );
        self.logical_dpi = dpi;
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

    /// Returns the generation of the installed text shaper.
    ///
    /// Widgets can use this to invalidate layouts produced by a previous
    /// shaper without depending on a concrete shaper implementation.
    #[must_use]
    pub const fn text_shaper_generation(&self) -> u64 {
        self.text_shaper_generation
    }

    /// Supplies the text shaper used by widgets that shape text at runtime.
    pub fn set_text_shaper(&mut self, text_shaper: impl TextShaper + 'static) {
        self.text_shaper = Box::new(text_shaper);
        self.text_shaper_generation = self.text_shaper_generation.saturating_add(1);
    }

    /// Restores Drawen's bundled default text shaper.
    pub fn clear_text_shaper(&mut self) {
        self.text_shaper = Box::new(FontdueTextShaper::ubuntu_light());
        self.text_shaper_generation = self.text_shaper_generation.saturating_add(1);
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

    /// Returns whether hover, press, and focus transitions are animated.
    #[must_use]
    pub const fn animations_enabled(&self) -> bool {
        self.animations_enabled
    }

    /// Enables or disables interaction animations.
    ///
    /// Applications can set this to `false` to honor a reduced-motion
    /// preference. Disabling animation makes every transition snap directly to
    /// its target state on the next frame.
    pub const fn set_animations_enabled(&mut self, enabled: bool) {
        self.animations_enabled = enabled;
    }
}

impl Default for UiEnvironment {
    fn default() -> Self {
        Self::new(LightTheme)
    }
}

#[cfg(test)]
mod tests {
    use drawen_core::Color;
    use drawen_render::{Font, FontFamily, FontSize, FontStyle, FontWeight};

    use super::UiEnvironment;
    use crate::{DssStyle, LightTheme, StyleTarget, Theme};

    #[test]
    fn constructs_a_theme_from_dss_source() {
        let environment = UiEnvironment::from_dss(
            LightTheme,
            ":root { color: #112233; } button { background: #445566; }",
        )
        .expect("valid DSS");

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

    #[test]
    fn applies_style_font_values_over_the_application_default() {
        let mut environment = UiEnvironment::new(LightTheme);
        environment.set_default_font(Font::from_family("Ubuntu").with_pixel_size(13.0));

        let style = DssStyle {
            font_families: Some(vec![FontFamily::new("Inter")]),
            font_size: Some(FontSize::points(12.0)),
            font_weight: Some(FontWeight::BOLD),
            font_style: Some(FontStyle::Italic),
            ..DssStyle::default()
        };

        let font = environment.font_for(&style);
        assert_eq!(font.family().map(FontFamily::as_str), Some("Inter"));
        assert!((font.pixel_size(96.0) - 16.0).abs() < f32::EPSILON);
        assert_eq!(font.weight(), FontWeight::BOLD);
        assert_eq!(font.style(), FontStyle::Italic);
        assert!((environment.font_size_for(&style) - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn resolves_point_sizes_at_the_environment_dpi() {
        let mut environment = UiEnvironment::new(LightTheme);
        environment.set_logical_dpi(144.0);

        let style = DssStyle {
            font_size: Some(FontSize::points(12.0)),
            ..DssStyle::default()
        };

        assert!((environment.font_size_for(&style) - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn application_font_overrides_the_theme_font() {
        let mut environment = UiEnvironment::new(LightTheme);
        assert_eq!(environment.default_font().family(), None);

        environment.set_default_font(Font::from_family("Inter"));
        assert_eq!(
            environment.default_font().family().map(FontFamily::as_str),
            Some("Inter")
        );

        environment.clear_default_font();
        assert_eq!(environment.default_font().family(), None);
    }
}
