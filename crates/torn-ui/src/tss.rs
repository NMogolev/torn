use std::{fmt, str::FromStr, time::Duration};

use torn_core::{Color, Insets};
use torn_render::{DEFAULT_DPI, FontFamily, FontSize, FontStyle, FontWeight};

use crate::{Border, Theme};

/// A parsed TSS stylesheet.
///
/// TSS uses a deliberately small CSS-compatible subset for UI styling. The
/// first version supports type, id, class, and state selectors; the `:root`
/// rule; and typed visual properties. It does not yet support descendant
/// selectors, variables, or layout properties whose ownership belongs to a
/// parent widget.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleSheet {
    rules: Vec<StyleRule>,
    /// Rule indices sorted by `(specificity, source order)` once at parse time.
    ///
    /// Keeping this index avoids sorting and allocating on every node style
    /// resolution. The public rule list remains in source order.
    cascade: Vec<usize>,
}

impl StyleSheet {
    /// Parses a stylesheet from UTF-8 TSS source.
    ///
    /// Unknown properties, unsupported selector syntax, and invalid values are
    /// rejected rather than silently ignored. This keeps stylesheet mistakes
    /// visible while TSS is still intentionally small.
    ///
    /// # Errors
    ///
    /// Returns [`TssError`] when `source` is not valid TSS.
    pub fn parse(source: &str) -> Result<Self, TssError> {
        let mut stylesheet = Parser::new(source).parse_stylesheet()?;
        stylesheet.rebuild_cascade();
        Ok(stylesheet)
    }

    /// Returns the stylesheet rules in source order.
    #[must_use]
    pub fn rules(&self) -> &[StyleRule] {
        &self.rules
    }

    /// Resolves all matching rules for `target` using CSS specificity and
    /// source order.
    ///
    /// The returned style is sparse: a missing value means that no matching
    /// rule supplied it. Widgets can therefore use a local value, the resolved
    /// TSS value, or their semantic theme default in that order.
    #[must_use]
    pub fn resolve(&self, target: &StyleTarget) -> TssStyle {
        let mut resolved = TssStyle::default();
        for &index in &self.cascade {
            let rule = &self.rules[index];
            if rule.selector.matches(target) {
                resolved.apply(&rule.style);
            }
        }
        resolved
    }

    fn rebuild_cascade(&mut self) {
        self.cascade = (0..self.rules.len()).collect();
        self.cascade.sort_by_key(|index| {
            let rule = &self.rules[*index];
            (rule.selector.specificity(), *index)
        });
    }

    /// Resolves the sheet into a [`Theme`] while preserving missing values from
    /// `fallback`.
    ///
    /// `:root` supplies global semantic values. Control-specific selectors are
    /// resolved per node by the runtime and are intentionally not mapped onto
    /// the semantic theme.
    #[must_use]
    pub fn theme<T: Theme>(&self, fallback: T) -> TssTheme<T> {
        TssTheme::new(fallback, self)
    }
}

impl FromStr for StyleSheet {
    type Err = TssError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::parse(source)
    }
}

/// A single selector and declaration block from a [`StyleSheet`].
#[derive(Clone, Debug, PartialEq)]
pub struct StyleRule {
    selector: Selector,
    style: TssStyle,
}

impl StyleRule {
    /// Returns the selector that identifies matching targets.
    #[must_use]
    pub const fn selector(&self) -> &Selector {
        &self.selector
    }

    /// Returns the declarations defined by this rule.
    #[must_use]
    pub const fn style(&self) -> &TssStyle {
        &self.style
    }
}

/// A compound TSS selector.
///
/// A selector may contain one type name, one id, any number of classes, and
/// zero or more pseudo-classes. For example, `button.primary:hover` is a
/// single compound selector. Combinators between nodes are deliberately not
/// supported yet.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Selector {
    is_root: bool,
    type_name: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    pseudo_classes: Vec<PseudoClass>,
}

impl Selector {
    /// Returns whether this selector matches `target`.
    #[must_use]
    pub fn matches(&self, target: &StyleTarget) -> bool {
        if self.is_root != target.is_root {
            return false;
        }
        if self.is_root {
            return true;
        }
        if self
            .type_name
            .as_deref()
            .is_some_and(|type_name| target.type_name.as_deref() != Some(type_name))
        {
            return false;
        }
        if self
            .id
            .as_deref()
            .is_some_and(|id| target.id.as_deref() != Some(id))
        {
            return false;
        }
        self.classes
            .iter()
            .all(|class| target.classes.iter().any(|candidate| candidate == class))
            && self
                .pseudo_classes
                .iter()
                .all(|state| target.states.contains(state))
    }

    /// Returns the selector specificity as `(ids, classes_and_states, types)`.
    #[must_use]
    pub fn specificity(&self) -> (usize, usize, usize) {
        (
            usize::from(self.id.is_some()),
            self.classes.len() + self.pseudo_classes.len() + usize::from(self.is_root),
            usize::from(self.type_name.is_some()),
        )
    }
}

/// Style metadata used to match a [`Selector`].
///
/// Applications will eventually assign this metadata to retained runtime
/// nodes. It is public now so custom widgets and tools can resolve a
/// [`StyleSheet`] using the exact same matching contract.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct StyleTarget {
    is_root: bool,
    type_name: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    states: Vec<PseudoClass>,
}

impl StyleTarget {
    /// Creates a target with the given application-defined type name.
    ///
    /// Type names are matched case-insensitively. Use [`Self::empty`] for a
    /// widget that deliberately has no type name and should match only
    /// universal and metadata selectors.
    #[must_use]
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: Some(type_name.into().to_ascii_lowercase()),
            ..Self::default()
        }
    }

    /// Creates a target with no type name, id, classes, or states.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            is_root: false,
            type_name: None,
            id: None,
            classes: Vec::new(),
            states: Vec::new(),
        }
    }

    /// Returns whether this is the `:root` target.
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.is_root
    }

    /// Returns the target's type name, if any.
    #[must_use]
    pub fn type_name(&self) -> Option<&str> {
        self.type_name.as_deref()
    }

    /// Returns the target's id, if any.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Returns the target's classes in insertion order.
    #[must_use]
    pub fn classes(&self) -> &[String] {
        &self.classes
    }

    /// Returns the target's active pseudo-classes in insertion order.
    #[must_use]
    pub fn states(&self) -> &[PseudoClass] {
        &self.states
    }

    /// Replaces the target's type name.
    #[must_use]
    pub fn with_type_name(mut self, type_name: impl Into<String>) -> Self {
        self.type_name = Some(type_name.into().to_ascii_lowercase());
        self
    }

    /// Creates the document root target matched by the `:root` selector.
    #[must_use]
    pub const fn root() -> Self {
        Self {
            is_root: true,
            type_name: None,
            id: None,
            classes: Vec::new(),
            states: Vec::new(),
        }
    }

    /// Adds an id matched by `#id` selectors.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Adds a class matched by `.class` selectors.
    #[must_use]
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        let class = class.into();
        if !self.classes.contains(&class) {
            self.classes.push(class);
        }
        self
    }

    /// Adds an active pseudo-class matched by `:state` selectors.
    #[must_use]
    pub fn with_state(mut self, state: PseudoClass) -> Self {
        if !self.states.contains(&state) {
            self.states.push(state);
        }
        self
    }
}

/// Interaction state supported by TSS pseudo-class selectors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PseudoClass {
    /// The pointer is over the target.
    Hover,
    /// The target is currently active or pressed.
    Pressed,
    /// The target owns keyboard focus.
    Focus,
    /// The target cannot be interacted with.
    Disabled,
}

/// Typed visual values produced by the TSS cascade.
///
/// Every field is optional so a rule can override exactly one property without
/// resetting values supplied by a less specific rule or a semantic theme.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TssStyle {
    /// Surface fill from `background` or `background-color`.
    pub background: Option<Color>,
    /// Text and icon color from `color`.
    pub foreground: Option<Color>,
    /// Focus and selection color from `accent-color`.
    pub accent: Option<Color>,
    /// Surface border from `border`.
    pub border: Option<Border>,
    /// Inner surface inset from `padding`.
    pub padding: Option<Insets>,
    /// Standard sibling spacing from `gap`.
    pub spacing: Option<f32>,
    /// Ordered family preferences from `font-family`.
    pub font_families: Option<Vec<FontFamily>>,
    /// Text size from `font-size`.
    pub font_size: Option<FontSize>,
    /// Text weight from `font-weight`.
    pub font_weight: Option<FontWeight>,
    /// Text slant from `font-style`.
    pub font_style: Option<FontStyle>,
    /// Surface corner radius from `border-radius`.
    pub corner_radius: Option<f32>,
}

impl TssStyle {
    fn apply(&mut self, style: &Self) {
        if style.background.is_some() {
            self.background = style.background;
        }
        if style.foreground.is_some() {
            self.foreground = style.foreground;
        }
        if style.accent.is_some() {
            self.accent = style.accent;
        }
        if style.border.is_some() {
            self.border = style.border;
        }
        if style.padding.is_some() {
            self.padding = style.padding;
        }
        if style.spacing.is_some() {
            self.spacing = style.spacing;
        }
        if style.font_families.is_some() {
            self.font_families.clone_from(&style.font_families);
        }
        if style.font_size.is_some() {
            self.font_size = style.font_size;
        }
        if style.font_weight.is_some() {
            self.font_weight = style.font_weight;
        }
        if style.font_style.is_some() {
            self.font_style = style.font_style;
        }
        if style.corner_radius.is_some() {
            self.corner_radius = style.corner_radius;
        }
    }

    /// Fills inheritable properties that this style does not define.
    ///
    /// `color`, `font-family`, `font-size`, `font-weight`, `font-style`, and
    /// `accent-color` inherit from a parent node. Surface properties such as
    /// `background`, `border`, `padding`, `gap`, and `border-radius` do not.
    pub fn inherit_from(&mut self, parent: &Self) {
        self.foreground = self.foreground.or(parent.foreground);
        if self.font_families.is_none() {
            self.font_families.clone_from(&parent.font_families);
        }
        self.font_size = self.font_size.or(parent.font_size);
        self.font_weight = self.font_weight.or(parent.font_weight);
        self.font_style = self.font_style.or(parent.font_style);
        self.accent = self.accent.or(parent.accent);
    }

    /// Returns whether no declaration produced a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.background.is_none()
            && self.foreground.is_none()
            && self.accent.is_none()
            && self.border.is_none()
            && self.padding.is_none()
            && self.spacing.is_none()
            && self.font_families.is_none()
            && self.font_size.is_none()
            && self.font_weight.is_none()
            && self.font_style.is_none()
            && self.corner_radius.is_none()
    }

    /// Returns `background`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn background_or(&self, fallback: Color) -> Color {
        self.background.unwrap_or(fallback)
    }

    /// Returns `color`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn foreground_or(&self, fallback: Color) -> Color {
        self.foreground.unwrap_or(fallback)
    }

    /// Returns `accent-color`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn accent_or(&self, fallback: Color) -> Color {
        self.accent.unwrap_or(fallback)
    }

    /// Returns `border`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn border_or(&self, fallback: Option<Border>) -> Option<Border> {
        self.border.or(fallback)
    }

    /// Returns `padding`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn padding_or(&self, fallback: Insets) -> Insets {
        self.padding.unwrap_or(fallback)
    }

    /// Returns `gap`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn spacing_or(&self, fallback: f32) -> f32 {
        self.spacing.unwrap_or(fallback)
    }

    /// Resolves `font-size` to logical pixels at `dpi`.
    #[must_use]
    pub fn font_size_pixels(&self, dpi: f32) -> Option<f32> {
        self.font_size.map(|size| size.pixels_at(dpi))
    }

    /// Returns `font-size` in logical pixels at [`DEFAULT_DPI`], or `fallback`.
    #[must_use]
    pub fn font_size_or(&self, fallback: f32) -> f32 {
        self.font_size_pixels(DEFAULT_DPI).unwrap_or(fallback)
    }

    /// Returns `font-weight`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn font_weight_or(&self, fallback: FontWeight) -> FontWeight {
        self.font_weight.unwrap_or(fallback)
    }

    /// Returns `font-style`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn font_style_or(&self, fallback: FontStyle) -> FontStyle {
        self.font_style.unwrap_or(fallback)
    }

    /// Returns `border-radius`, or `fallback` when TSS did not supply it.
    #[must_use]
    pub fn corner_radius_or(&self, fallback: f32) -> f32 {
        self.corner_radius.unwrap_or(fallback)
    }
}

/// A [`Theme`] whose values are resolved from a [`StyleSheet`].
///
/// The stylesheet remains the source of truth at construction time. Values not
/// supplied by TSS continue to come from `T`, preserving custom themes and the
/// appearance of existing applications.
#[derive(Clone, Debug)]
pub struct TssTheme<T> {
    fallback: T,
    root: TssStyle,
}

impl<T> TssTheme<T> {
    /// Returns the semantic theme used for properties absent from TSS.
    #[must_use]
    pub const fn fallback(&self) -> &T {
        &self.fallback
    }
}

impl<T: Theme> TssTheme<T> {
    /// Resolves `stylesheet` against the standard theme targets.
    #[must_use]
    pub fn new(fallback: T, stylesheet: &StyleSheet) -> Self {
        Self {
            fallback,
            root: stylesheet.resolve(&StyleTarget::root()),
        }
    }
}

impl<T: Theme> Theme for TssTheme<T> {
    fn background(&self) -> Color {
        self.root
            .background
            .unwrap_or_else(|| self.fallback.background())
    }

    fn foreground(&self) -> Color {
        self.root
            .foreground
            .unwrap_or_else(|| self.fallback.foreground())
    }

    fn accent(&self) -> Color {
        self.root.accent.unwrap_or_else(|| self.fallback.accent())
    }

    fn surface(&self) -> Color {
        self.root
            .background
            .unwrap_or_else(|| self.fallback.surface())
    }

    fn raised_surface(&self) -> Color {
        self.fallback.raised_surface()
    }

    fn border(&self) -> Color {
        self.root
            .border
            .map_or_else(|| self.fallback.border(), |border| border.color)
    }

    fn focus_ring(&self) -> Color {
        self.root
            .accent
            .unwrap_or_else(|| self.fallback.focus_ring())
    }

    fn shadow(&self) -> Color {
        self.fallback.shadow()
    }

    fn accent_foreground(&self) -> Color {
        self.fallback.accent_foreground()
    }

    fn selection(&self) -> Color {
        self.root.accent.map_or_else(
            || self.fallback.selection(),
            |accent| accent.with_alpha(0.24),
        )
    }

    fn disabled_foreground(&self) -> Color {
        self.fallback.disabled_foreground()
    }

    fn control_background(&self) -> Color {
        self.fallback.control_background()
    }

    fn control_pressed_background(&self) -> Color {
        self.fallback.control_pressed_background()
    }

    fn control_hover_background(&self) -> Color {
        self.fallback.control_hover_background()
    }

    fn control_padding(&self) -> Insets {
        self.fallback.control_padding()
    }

    fn spacing(&self) -> f32 {
        self.root.spacing.unwrap_or_else(|| self.fallback.spacing())
    }

    fn font_size(&self) -> f32 {
        self.root
            .font_size_pixels(DEFAULT_DPI)
            .unwrap_or_else(|| self.fallback.font_size())
    }

    fn font_family(&self) -> Option<&str> {
        self.root
            .font_families
            .as_ref()
            .and_then(|families| families.first())
            .map(FontFamily::as_str)
            .or_else(|| self.fallback.font_family())
    }

    fn font_weight(&self) -> FontWeight {
        self.root
            .font_weight
            .unwrap_or_else(|| self.fallback.font_weight())
    }

    fn font_style(&self) -> FontStyle {
        self.root
            .font_style
            .unwrap_or_else(|| self.fallback.font_style())
    }

    fn corner_radius(&self) -> f32 {
        self.root
            .corner_radius
            .unwrap_or_else(|| self.fallback.corner_radius())
    }

    fn animation_duration(&self) -> Duration {
        self.fallback.animation_duration()
    }
}

/// An error returned while parsing TSS source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TssError {
    position: usize,
    message: String,
}

impl TssError {
    /// Returns the zero-based UTF-8 byte offset at which parsing failed.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Returns a human-readable explanation of the parsing error.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for TssError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "TSS parse error at byte {}: {}",
            self.position, self.message
        )
    }
}

impl std::error::Error for TssError {}

struct Parser<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Parser<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    fn parse_stylesheet(mut self) -> Result<StyleSheet, TssError> {
        let mut rules = Vec::new();
        self.skip_ignored()?;
        while !self.is_eof() {
            let selector_position = self.cursor;
            let selector_source = self.read_until_open_brace()?;
            let selectors = parse_selectors(selector_source, selector_position)?;
            self.advance_char();
            let style = self.parse_declarations()?;
            rules.extend(selectors.into_iter().map(|selector| StyleRule {
                selector,
                style: style.clone(),
            }));
            self.skip_ignored()?;
        }
        Ok(StyleSheet {
            rules,
            cascade: Vec::new(),
        })
    }

    fn parse_declarations(&mut self) -> Result<TssStyle, TssError> {
        let mut style = TssStyle::default();
        loop {
            self.skip_ignored()?;
            if self.consume_char('}') {
                return Ok(style);
            }
            if self.is_eof() {
                return Err(self.error("expected `}` to close declaration block"));
            }

            let property_position = self.cursor;
            let property = self.read_identifier()?;
            self.skip_ignored()?;
            if !self.consume_char(':') {
                return Err(self.error("expected `:` after property name"));
            }
            self.skip_ignored()?;
            let value_position = self.cursor;
            let value = self.read_declaration_value()?;
            if value.trim().is_empty() {
                return Err(TssError {
                    position: value_position,
                    message: "expected a property value".to_owned(),
                });
            }
            set_property(&mut style, &property, value.trim(), value_position)?;

            if self.consume_char(';') {
                continue;
            }
            if self.consume_char('}') {
                return Ok(style);
            }
            return Err(TssError {
                position: property_position,
                message: "expected `;` or `}` after property value".to_owned(),
            });
        }
    }

    fn skip_ignored(&mut self) -> Result<(), TssError> {
        loop {
            while self.peek_char().is_some_and(char::is_whitespace) {
                self.advance_char();
            }
            if !self.remaining().starts_with("/*") {
                return Ok(());
            }
            let comment_position = self.cursor;
            self.cursor += 2;
            let Some(end) = self.remaining().find("*/") else {
                return Err(TssError {
                    position: comment_position,
                    message: "unterminated comment".to_owned(),
                });
            };
            self.cursor += end + 2;
        }
    }

    fn read_until_open_brace(&mut self) -> Result<&'a str, TssError> {
        let start = self.cursor;
        while let Some(character) = self.peek_char() {
            match character {
                '{' => return Ok(&self.source[start..self.cursor]),
                '}' => return Err(self.error("unexpected `}` outside declaration block")),
                _ => self.advance_char(),
            }
        }
        Err(self.error("expected `{` after selector"))
    }

    fn read_identifier(&mut self) -> Result<String, TssError> {
        let start = self.cursor;
        while self.peek_char().is_some_and(is_identifier_character) {
            self.advance_char();
        }
        if start == self.cursor {
            return Err(self.error("expected identifier"));
        }
        Ok(self.source[start..self.cursor].to_ascii_lowercase())
    }

    fn read_declaration_value(&mut self) -> Result<&'a str, TssError> {
        let start = self.cursor;
        let mut parentheses = 0_usize;
        while let Some(character) = self.peek_char() {
            match character {
                '(' => {
                    parentheses += 1;
                    self.advance_char();
                }
                ')' if parentheses > 0 => {
                    parentheses -= 1;
                    self.advance_char();
                }
                ')' => return Err(self.error("unexpected `)` in property value")),
                ';' | '}' if parentheses == 0 => return Ok(&self.source[start..self.cursor]),
                _ => self.advance_char(),
            }
        }
        if parentheses > 0 {
            return Err(self.error("unterminated function in property value"));
        }
        Err(self.error("expected `;` or `}` after property value"))
    }

    fn consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.advance_char();
            true
        } else {
            false
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance_char(&mut self) {
        if let Some(character) = self.peek_char() {
            self.cursor += character.len_utf8();
        }
    }

    const fn is_eof(&self) -> bool {
        self.cursor == self.source.len()
    }

    fn remaining(&self) -> &str {
        &self.source[self.cursor..]
    }

    fn error(&self, message: impl Into<String>) -> TssError {
        TssError {
            position: self.cursor,
            message: message.into(),
        }
    }
}

fn parse_selectors(source: &str, position: usize) -> Result<Vec<Selector>, TssError> {
    let mut selectors = Vec::new();
    let mut offset = 0;
    for selector_source in source.split(',') {
        let selector = selector_source.trim();
        let leading_whitespace = selector_source.len() - selector_source.trim_start().len();
        selectors.push(parse_selector(
            selector,
            position + offset + leading_whitespace,
        )?);
        offset += selector_source.len() + 1;
    }
    Ok(selectors)
}

fn parse_selector(source: &str, position: usize) -> Result<Selector, TssError> {
    if source.is_empty() {
        return Err(TssError {
            position,
            message: "expected selector".to_owned(),
        });
    }
    if source == ":root" {
        return Ok(Selector {
            is_root: true,
            ..Selector::default()
        });
    }

    let mut selector = Selector::default();
    let mut cursor = 0;
    let mut saw_universal = false;
    while cursor < source.len() {
        let character = source[cursor..]
            .chars()
            .next()
            .expect("cursor is within selector source");
        match character {
            '*' if cursor == 0 => {
                saw_universal = true;
                cursor += 1;
            }
            '.' => {
                cursor += 1;
                let (class, next) = read_selector_identifier(source, cursor, position)?;
                if !selector.classes.contains(&class) {
                    selector.classes.push(class);
                }
                cursor = next;
            }
            '#' => {
                if selector.id.is_some() {
                    return Err(selector_error(
                        position + cursor,
                        "selector may contain only one id",
                    ));
                }
                cursor += 1;
                let (id, next) = read_selector_identifier(source, cursor, position)?;
                selector.id = Some(id);
                cursor = next;
            }
            ':' => {
                cursor += 1;
                let (pseudo_class, next) = read_selector_identifier(source, cursor, position)?;
                let state = match pseudo_class.as_str() {
                    "hover" => PseudoClass::Hover,
                    "pressed" => PseudoClass::Pressed,
                    "focus" => PseudoClass::Focus,
                    "disabled" => PseudoClass::Disabled,
                    _ => {
                        return Err(selector_error(
                            position + cursor,
                            format!("unsupported pseudo-class `:{pseudo_class}`"),
                        ));
                    }
                };
                if !selector.pseudo_classes.contains(&state) {
                    selector.pseudo_classes.push(state);
                }
                cursor = next;
            }
            _ if is_identifier_character(character)
                && selector.type_name.is_none()
                && !saw_universal =>
            {
                let (type_name, next) = read_selector_identifier(source, cursor, position)?;
                selector.type_name = Some(type_name.to_ascii_lowercase());
                cursor = next;
            }
            _ => {
                return Err(selector_error(
                    position + cursor,
                    "unsupported selector syntax",
                ));
            }
        }
    }
    Ok(selector)
}

fn read_selector_identifier(
    source: &str,
    cursor: usize,
    position: usize,
) -> Result<(String, usize), TssError> {
    let remainder = &source[cursor..];
    let length = remainder
        .char_indices()
        .take_while(|(_, character)| is_identifier_character(*character))
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    if length == 0 {
        return Err(selector_error(
            position + cursor,
            "expected selector identifier",
        ));
    }
    Ok((remainder[..length].to_owned(), cursor + length))
}

fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
}

fn selector_error(position: usize, message: impl Into<String>) -> TssError {
    TssError {
        position,
        message: message.into(),
    }
}

fn set_property(
    style: &mut TssStyle,
    property: &str,
    value: &str,
    position: usize,
) -> Result<(), TssError> {
    match property {
        "background" | "background-color" => style.background = Some(parse_color(value, position)?),
        "color" => style.foreground = Some(parse_color(value, position)?),
        "accent-color" => style.accent = Some(parse_color(value, position)?),
        "border" => style.border = Some(parse_border(value, position)?),
        "padding" => style.padding = Some(parse_padding(value, position)?),
        "gap" => style.spacing = Some(parse_length(value, position)?),
        "font-family" => style.font_families = Some(parse_font_families(value, position)?),
        "font-size" => style.font_size = Some(parse_font_size(value, position)?),
        "font-weight" => style.font_weight = Some(parse_font_weight(value, position)?),
        "font-style" => style.font_style = Some(parse_font_style(value, position)?),
        "border-radius" => style.corner_radius = Some(parse_length(value, position)?),
        _ => {
            return Err(TssError {
                position,
                message: format!("unsupported property `{property}`"),
            });
        }
    }
    Ok(())
}

fn parse_color(value: &str, position: usize) -> Result<Color, TssError> {
    if value.eq_ignore_ascii_case("transparent") {
        return Ok(Color::TRANSPARENT);
    }
    if value.eq_ignore_ascii_case("black") {
        return Ok(Color::BLACK);
    }
    if value.eq_ignore_ascii_case("white") {
        return Ok(Color::WHITE);
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex_color(hex, position);
    }
    if let Some(arguments) = value
        .strip_prefix("rgb(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let channels = parse_color_channels(arguments, 3, position)?;
        return Ok(Color::rgba8(channels[0], channels[1], channels[2], 255));
    }
    if let Some(arguments) = value
        .strip_prefix("rgba(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let channels = parse_color_channels(arguments, 4, position)?;
        return Ok(Color::rgba8(
            channels[0],
            channels[1],
            channels[2],
            channels[3],
        ));
    }
    Err(TssError {
        position,
        message: format!("invalid color `{value}`"),
    })
}

fn parse_hex_color(hex: &str, position: usize) -> Result<Color, TssError> {
    let components = match hex.len() {
        3 => [
            duplicate_hex_digit(hex, 0, position)?,
            duplicate_hex_digit(hex, 1, position)?,
            duplicate_hex_digit(hex, 2, position)?,
            255,
        ],
        4 => [
            duplicate_hex_digit(hex, 0, position)?,
            duplicate_hex_digit(hex, 1, position)?,
            duplicate_hex_digit(hex, 2, position)?,
            duplicate_hex_digit(hex, 3, position)?,
        ],
        6 => [
            parse_hex_pair(hex, 0, position)?,
            parse_hex_pair(hex, 2, position)?,
            parse_hex_pair(hex, 4, position)?,
            255,
        ],
        8 => [
            parse_hex_pair(hex, 0, position)?,
            parse_hex_pair(hex, 2, position)?,
            parse_hex_pair(hex, 4, position)?,
            parse_hex_pair(hex, 6, position)?,
        ],
        _ => {
            return Err(TssError {
                position,
                message: "hex colors must contain 3, 4, 6, or 8 digits".to_owned(),
            });
        }
    };
    Ok(Color::rgba8(
        components[0],
        components[1],
        components[2],
        components[3],
    ))
}

fn duplicate_hex_digit(hex: &str, index: usize, position: usize) -> Result<u8, TssError> {
    let digit = hex
        .as_bytes()
        .get(index)
        .copied()
        .and_then(hex_value)
        .ok_or_else(|| TssError {
            position: position + 1 + index,
            message: "invalid hexadecimal color digit".to_owned(),
        })?;
    Ok(digit * 17)
}

fn parse_hex_pair(hex: &str, index: usize, position: usize) -> Result<u8, TssError> {
    u8::from_str_radix(&hex[index..index + 2], 16).map_err(|_| TssError {
        position: position + 1 + index,
        message: "invalid hexadecimal color digit".to_owned(),
    })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_color_channels(
    arguments: &str,
    expected: usize,
    position: usize,
) -> Result<Vec<u8>, TssError> {
    let channels: Vec<&str> = arguments.split(',').map(str::trim).collect();
    if channels.len() != expected {
        return Err(TssError {
            position,
            message: format!("expected {expected} color channels"),
        });
    }
    channels
        .into_iter()
        .enumerate()
        .map(|(index, channel)| {
            if expected == 4 && index == 3 {
                parse_alpha(channel, position)
            } else {
                channel.parse::<u8>().map_err(|_| TssError {
                    position,
                    message: format!("invalid color channel `{channel}`"),
                })
            }
        })
        .collect()
}

fn parse_alpha(value: &str, position: usize) -> Result<u8, TssError> {
    let alpha = if let Some(percent) = value.strip_suffix('%') {
        let percent = percent.parse::<f32>().map_err(|_| TssError {
            position,
            message: format!("invalid alpha channel `{value}`"),
        })?;
        percent / 100.0
    } else {
        value.parse::<f32>().map_err(|_| TssError {
            position,
            message: format!("invalid alpha channel `{value}`"),
        })?
    };
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(TssError {
            position,
            message: "alpha channel must be between 0 and 1".to_owned(),
        });
    }
    Ok(round_alpha_channel(alpha))
}

fn round_alpha_channel(alpha: f32) -> u8 {
    // `parse_alpha` admits only finite values in the inclusive 0.0..=1.0
    // range, so the rounded result is necessarily an unsigned byte.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (alpha * 255.0).round() as u8
    }
}

fn parse_border(value: &str, position: usize) -> Result<Border, TssError> {
    // Split only the first two whitespace-delimited tokens. The remainder is
    // the color and may itself contain spaces, as in
    // `2px solid rgba(12, 34, 56, 0.5)`.
    let value = value.trim();
    let Some((width, rest)) = value.split_once(char::is_whitespace) else {
        return Err(TssError {
            position,
            message: "border must be `<width> solid <color>`".to_owned(),
        });
    };
    let rest = rest.trim_start();
    let Some((line_style, color)) = rest.split_once(char::is_whitespace) else {
        return Err(TssError {
            position,
            message: "border must be `<width> solid <color>`".to_owned(),
        });
    };
    if !line_style.eq_ignore_ascii_case("solid") {
        return Err(TssError {
            position,
            message: "only `solid` borders are supported".to_owned(),
        });
    }
    let color = color.trim();
    if color.is_empty() {
        return Err(TssError {
            position,
            message: "border must be `<width> solid <color>`".to_owned(),
        });
    }
    Ok(Border::new(
        parse_length(width, position)?,
        parse_color(color, position)?,
    ))
}

fn parse_padding(value: &str, position: usize) -> Result<Insets, TssError> {
    let values: Result<Vec<f32>, TssError> = value
        .split_ascii_whitespace()
        .map(|part| parse_length(part, position))
        .collect();
    let values = values?;
    match values.as_slice() {
        [all] => Ok(Insets::all(*all)),
        [vertical, horizontal] => Ok(Insets::new(*vertical, *horizontal, *vertical, *horizontal)),
        [top, horizontal, bottom] => Ok(Insets::new(*top, *horizontal, *bottom, *horizontal)),
        [top, right, bottom, left] => Ok(Insets::new(*top, *right, *bottom, *left)),
        _ => Err(TssError {
            position,
            message: "padding requires between one and four lengths".to_owned(),
        }),
    }
}

fn parse_font_families(value: &str, position: usize) -> Result<Vec<FontFamily>, TssError> {
    let mut families = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(TssError {
                position,
                message: "font-family requires non-empty family names".to_owned(),
            });
        }
        let name = unquote_font_family(part);
        if name.is_empty() {
            return Err(TssError {
                position,
                message: "font-family requires non-empty family names".to_owned(),
            });
        }
        families.push(FontFamily::new(name));
    }
    if families.is_empty() {
        return Err(TssError {
            position,
            message: "font-family requires at least one family name".to_owned(),
        });
    }
    Ok(families)
}

fn unquote_font_family(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn parse_font_size(value: &str, position: usize) -> Result<FontSize, TssError> {
    let value = value.trim();
    if let Some(number) = value.strip_suffix("px") {
        return Ok(FontSize::pixels(parse_font_number(
            number.trim(),
            value,
            position,
        )?));
    }
    if let Some(number) = value.strip_suffix("pt") {
        return Ok(FontSize::points(parse_font_number(
            number.trim(),
            value,
            position,
        )?));
    }
    if value == "0" {
        return Ok(FontSize::pixels(0.0));
    }
    Err(TssError {
        position,
        message: "font-size must be a non-negative `px` or `pt` length".to_owned(),
    })
}

fn parse_font_number(number: &str, value: &str, position: usize) -> Result<f32, TssError> {
    let number = number.parse::<f32>().map_err(|_| TssError {
        position,
        message: format!("invalid font length `{value}`"),
    })?;
    if !number.is_finite() || number < 0.0 {
        return Err(TssError {
            position,
            message: "font length must be finite and non-negative".to_owned(),
        });
    }
    Ok(number)
}

fn parse_font_weight(value: &str, position: usize) -> Result<FontWeight, TssError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "normal" => Ok(FontWeight::NORMAL),
        "bold" => Ok(FontWeight::BOLD),
        _ => {
            let weight = normalized.parse::<u16>().map_err(|_| TssError {
                position,
                message: format!("invalid font-weight `{value}`"),
            })?;
            if !(100..=900).contains(&weight) {
                return Err(TssError {
                    position,
                    message: "font-weight must be `normal`, `bold`, or 100 through 900".to_owned(),
                });
            }
            FontWeight::new(weight).ok_or(TssError {
                position,
                message: "font-weight is outside the supported range".to_owned(),
            })
        }
    }
}

fn parse_font_style(value: &str, position: usize) -> Result<FontStyle, TssError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Ok(FontStyle::Normal),
        "italic" => Ok(FontStyle::Italic),
        "oblique" => Ok(FontStyle::Oblique),
        _ => Err(TssError {
            position,
            message: format!("invalid font-style `{value}`"),
        }),
    }
}

fn parse_length(value: &str, position: usize) -> Result<f32, TssError> {
    let value = value.trim();
    let (number, unitless) = match value.strip_suffix("px") {
        Some(number) => (number.trim(), false),
        None => (value, true),
    };
    let length = number.parse::<f32>().map_err(|_| TssError {
        position,
        message: format!("invalid length `{value}`"),
    })?;
    if !length.is_finite() || length < 0.0 {
        return Err(TssError {
            position,
            message: "length must be finite and non-negative".to_owned(),
        });
    }
    if unitless && length != 0.0 {
        return Err(TssError {
            position,
            message: "non-zero lengths must use the `px` unit".to_owned(),
        });
    }
    Ok(length)
}

#[cfg(test)]
mod tests {
    use torn_core::Insets;

    use crate::{Border, LightTheme, Theme};

    use super::{Color, FontSize, PseudoClass, StyleSheet, StyleTarget};

    fn resolve(source: &str, target: &StyleTarget) -> super::TssStyle {
        StyleSheet::parse(source)
            .expect("test stylesheet is valid")
            .resolve(target)
    }

    #[test]
    fn resolves_the_most_specific_matching_declarations() {
        let stylesheet = StyleSheet::parse(
            r"
                * { color: #111; font-size: 13px; }
                button { background: #224466; padding: 4px 8px; }
                .primary { background: #336699; }
                #save.primary:hover { background: rgba(12, 34, 56, 0.5); }
            ",
        )
        .expect("valid TSS");
        let target = StyleTarget::new("button")
            .with_id("save")
            .with_class("primary")
            .with_state(PseudoClass::Hover);

        let style = stylesheet.resolve(&target);
        assert_eq!(style.foreground, Some(Color::rgba8(17, 17, 17, 255)));
        assert_eq!(style.background, Some(Color::rgba8(12, 34, 56, 128)));
        assert_eq!(style.padding, Some(Insets::new(4.0, 8.0, 4.0, 8.0)));
        assert_eq!(style.font_size, Some(FontSize::pixels(13.0)));
    }

    #[test]
    fn source_order_breaks_specificity_ties() {
        let target = StyleTarget::new("button")
            .with_class("primary")
            .with_class("wide");
        let style = resolve(
            r"
                button.primary { background: #111111; }
                button.wide { background: #222222; }
            ",
            &target,
        );
        assert_eq!(style.background, Some(Color::rgba8(34, 34, 34, 255)));
    }

    #[test]
    fn root_supplies_theme_tokens_without_rewriting_control_values() {
        let stylesheet = StyleSheet::parse(
            r"
                :root {
                    background: #101820;
                    color: #f1f3f5;
                    accent-color: #2edcb3;
                    gap: 12px;
                    font-size: 15px;
                    border-radius: 6px;
                }
                button { background: #243447; padding: 6px 10px; }
                button:hover { background: #34516b; }
                button:pressed { background: #182838; }
            ",
        )
        .expect("valid TSS");
        let theme = stylesheet.theme(LightTheme);
        let button = stylesheet.resolve(&StyleTarget::new("button"));
        let hovered =
            stylesheet.resolve(&StyleTarget::new("button").with_state(PseudoClass::Hover));
        let pressed =
            stylesheet.resolve(&StyleTarget::new("button").with_state(PseudoClass::Pressed));

        assert_eq!(theme.background(), Color::rgba8(16, 24, 32, 255));
        assert_eq!(theme.foreground(), Color::rgba8(241, 243, 245, 255));
        assert_eq!(theme.accent(), Color::rgba8(46, 220, 179, 255));
        assert!((theme.spacing() - 12.0).abs() < f32::EPSILON);
        assert!((theme.font_size() - 15.0).abs() < f32::EPSILON);
        assert!((theme.corner_radius() - 6.0).abs() < f32::EPSILON);
        // Control selectors are node styles, not global theme overrides.
        assert_eq!(theme.control_background(), LightTheme.control_background());
        assert_eq!(button.background, Some(Color::rgba8(36, 52, 71, 255)));
        assert_eq!(hovered.background, Some(Color::rgba8(52, 81, 107, 255)));
        assert_eq!(pressed.background, Some(Color::rgba8(24, 40, 56, 255)));
        assert_eq!(button.padding, Some(Insets::new(6.0, 10.0, 6.0, 10.0)));
    }

    #[test]
    fn supports_comments_and_comma_separated_selectors() {
        let stylesheet = StyleSheet::parse(
            "/* shared state */ button:hover, .preview { border: 2px solid #abcdef; }",
        )
        .expect("valid TSS");
        let hover = stylesheet.resolve(&StyleTarget::new("button").with_state(PseudoClass::Hover));
        let preview = stylesheet.resolve(&StyleTarget::new("image").with_class("preview"));

        assert_eq!(hover.border, preview.border);
        assert!((hover.border.expect("border declaration").width - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parses_border_colors_that_contain_spaces() {
        let style = resolve(
            "button { border: 2px solid rgba(12, 34, 56, 0.5); }",
            &StyleTarget::new("button"),
        );
        assert_eq!(
            style.border,
            Some(Border::new(2.0, Color::rgba8(12, 34, 56, 128)))
        );
    }

    #[test]
    fn parses_all_documented_color_forms() {
        let cases = [
            ("transparent", Color::TRANSPARENT),
            ("black", Color::BLACK),
            ("white", Color::WHITE),
            ("#abc", Color::rgba8(0xaa, 0xbb, 0xcc, 255)),
            ("#abcd", Color::rgba8(0xaa, 0xbb, 0xcc, 0xdd)),
            ("#a1b2c3", Color::rgba8(0xa1, 0xb2, 0xc3, 255)),
            ("#a1b2c3d4", Color::rgba8(0xa1, 0xb2, 0xc3, 0xd4)),
            ("rgb(1, 2, 3)", Color::rgba8(1, 2, 3, 255)),
            ("rgb( 1 , 2 , 3 )", Color::rgba8(1, 2, 3, 255)),
            ("rgba(1, 2, 3, 0.5)", Color::rgba8(1, 2, 3, 128)),
            ("rgba(1, 2, 3, 50%)", Color::rgba8(1, 2, 3, 128)),
            ("rgba(1, 2, 3, 0)", Color::rgba8(1, 2, 3, 0)),
        ];
        for (source, expected) in cases {
            let style = resolve(
                &format!("button {{ background: {source}; }}"),
                &StyleTarget::new("button"),
            );
            assert_eq!(style.background, Some(expected), "color `{source}`");
        }
    }

    #[test]
    fn parses_all_padding_shorthands() {
        let cases = [
            ("1px", Insets::all(1.0)),
            ("1px 2px", Insets::new(1.0, 2.0, 1.0, 2.0)),
            ("1px 2px 3px", Insets::new(1.0, 2.0, 3.0, 2.0)),
            ("1px 2px 3px 4px", Insets::new(1.0, 2.0, 3.0, 4.0)),
            ("0", Insets::all(0.0)),
        ];
        for (source, expected) in cases {
            let style = resolve(
                &format!("button {{ padding: {source}; }}"),
                &StyleTarget::new("button"),
            );
            assert_eq!(style.padding, Some(expected), "padding `{source}`");
        }
    }

    #[test]
    fn parses_font_family_size_weight_and_style() {
        let style = resolve(
            r#"button {
                font-family: "Ubuntu", 'Noto Sans', sans-serif;
                font-size: 18px;
                font-weight: 700;
                font-style: oblique;
            }"#,
            &StyleTarget::new("button"),
        );

        assert_eq!(
            style.font_families.as_deref(),
            Some(
                [
                    super::FontFamily::new("Ubuntu"),
                    super::FontFamily::new("Noto Sans"),
                    super::FontFamily::new("sans-serif"),
                ]
                .as_slice()
            )
        );
        assert_eq!(style.font_size, Some(FontSize::pixels(18.0)));
        assert_eq!(style.font_weight, Some(super::FontWeight::BOLD));
        assert_eq!(style.font_style, Some(super::FontStyle::Oblique));
    }

    #[test]
    fn resolves_point_sizes_and_font_weight_keywords() {
        let style = resolve(
            "text { font-size: 12pt; font-weight: normal; font-style: normal; }",
            &StyleTarget::new("text"),
        );
        assert_eq!(style.font_size, Some(FontSize::points(12.0)));
        assert!((style.font_size_pixels(96.0).expect("size") - 16.0).abs() < f32::EPSILON);
        assert_eq!(style.font_weight, Some(super::FontWeight::NORMAL));
        assert_eq!(style.font_style, Some(super::FontStyle::Normal));

        let bold = resolve("text { font-weight: bold; }", &StyleTarget::new("text"));
        assert_eq!(bold.font_weight, Some(super::FontWeight::BOLD));
    }

    #[test]
    fn font_properties_inherit_from_parent_styles() {
        let root = resolve(
            ":root { font-family: Root; font-size: 10px; font-weight: 300; font-style: italic; }",
            &StyleTarget::root(),
        );
        let mut child = super::TssStyle::default();
        child.inherit_from(&root);

        assert_eq!(child.font_families, root.font_families);
        assert_eq!(child.font_size, root.font_size);
        assert_eq!(child.font_weight, root.font_weight);
        assert_eq!(child.font_style, root.font_style);
    }

    #[test]
    fn rejects_invalid_font_values() {
        for (property, value) in [
            ("font-size", "12"),
            ("font-size", "-1px"),
            ("font-size", "large"),
            ("font-weight", "950"),
            ("font-weight", "heavy"),
            ("font-style", "slanted"),
            ("font-family", "\"\""),
            ("font-family", "Ubuntu,,Sans"),
        ] {
            let source = format!("button {{ {property}: {value}; }}");
            let error = StyleSheet::parse(&source)
                .expect_err(&format!("`{property}: {value}` must be rejected"));
            assert!(
                error.message().contains("font"),
                "unexpected error for `{property}: {value}`: {}",
                error.message()
            );
        }
    }

    #[test]
    fn accepts_crlf_and_unicode_comments() {
        let style = resolve(
            "/* привет 🦀 */\r\nbutton {\r\n  background: #102030;\r\n}\r\n",
            &StyleTarget::new("button"),
        );
        assert_eq!(style.background, Some(Color::rgba8(16, 32, 48, 255)));
    }

    #[test]
    fn rejects_unknown_properties_and_invalid_values() {
        let error =
            StyleSheet::parse("button { width: 24px; }").expect_err("property is unsupported");
        assert!(error.message().contains("unsupported property `width`"));
        assert_eq!(error.position(), 16);

        let error = StyleSheet::parse("button { padding: -1px; }").expect_err("negative padding");
        assert!(error.message().contains("non-negative"));

        let error = StyleSheet::parse("button { padding: 1px 2px 3px 4px 5px; }")
            .expect_err("five padding values are invalid");
        assert!(error.message().contains("between one and four"));
    }

    #[test]
    fn rejects_universal_selectors_followed_by_a_type_name() {
        let error = StyleSheet::parse("*button { background: #000; }")
            .expect_err("`*button` is not a type selector");
        assert!(error.message().contains("unsupported selector syntax"));

        assert!(StyleSheet::parse("*.primary { background: #000; }").is_ok());
        assert!(StyleSheet::parse("#id.primary:hover { background: #000; }").is_ok());
    }

    #[test]
    fn reports_the_position_of_selector_errors() {
        let error = StyleSheet::parse("button { background: #000; }\n:unknown { color: #fff; }")
            .expect_err("unknown pseudo-class");
        assert!(
            error
                .message()
                .contains("unsupported pseudo-class `:unknown`")
        );
        assert_eq!(error.position(), 30);
    }

    #[test]
    fn empty_sheet_and_universal_selectors_are_valid() {
        assert!(
            StyleSheet::parse("")
                .expect("empty sheet")
                .rules()
                .is_empty()
        );
        let style = resolve(
            "* { color: #010203; }",
            &StyleTarget::empty().with_class("anything"),
        );
        assert_eq!(style.foreground, Some(Color::rgba8(1, 2, 3, 255)));
    }

    #[test]
    fn random_utf8_source_never_panics() {
        const ALPHABET: &[char] = &[
            'a', 'Z', '0', '-', '_', '.', '#', ':', '*', '{', '}', ';', ',', '(', ')', '%', '#',
            ' ', '\t', '\r', '\n', '/', '\\', '"', '🦀', 'я', 'é',
        ];
        let alphabet_len = u32::try_from(ALPHABET.len()).expect("alphabet fits u32");
        let mut state = 0x1234_5678_u32;
        for _ in 0..2000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let length = usize::try_from(state % 64).expect("modulo fits usize");
            let mut source = String::new();
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let index = usize::try_from(state % alphabet_len).expect("modulo fits usize");
                source.push(ALPHABET[index]);
            }
            let result = std::panic::catch_unwind(|| StyleSheet::parse(&source));
            assert!(result.is_ok(), "parser panicked on source {source:?}");
        }
    }
}
