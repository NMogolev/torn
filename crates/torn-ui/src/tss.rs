use std::{fmt, str::FromStr};

use torn_core::{Color, Insets};

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
        Parser::new(source).parse_stylesheet()
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
        let mut rules: Vec<(usize, &StyleRule)> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| rule.selector.matches(target))
            .collect();
        rules.sort_by_key(|(source_order, rule)| (rule.selector.specificity(), *source_order));

        let mut resolved = TssStyle::default();
        for (_, rule) in rules {
            resolved.apply(rule.style);
        }
        resolved
    }

    /// Resolves the sheet into a [`Theme`] while preserving missing values from
    /// `fallback`.
    ///
    /// `:root` supplies global semantic values. `button`, `button:hover`, and
    /// `button:pressed` supply the existing button theme values, so all current
    /// standard widgets can use TSS without changing their APIs.
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
    pub const fn style(&self) -> TssStyle {
        self.style
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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StyleTarget {
    is_root: bool,
    type_name: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    states: Vec<PseudoClass>,
}

impl StyleTarget {
    /// Creates a target with the given application-defined type name.
    #[must_use]
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: Some(type_name.into().to_ascii_lowercase()),
            ..Self::default()
        }
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    /// Text size from `font-size`.
    pub font_size: Option<f32>,
    /// Surface corner radius from `border-radius`.
    pub corner_radius: Option<f32>,
}

impl TssStyle {
    fn apply(&mut self, style: Self) {
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
        if style.font_size.is_some() {
            self.font_size = style.font_size;
        }
        if style.corner_radius.is_some() {
            self.corner_radius = style.corner_radius;
        }
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
    button: TssStyle,
    hovered_button: TssStyle,
    pressed_button: TssStyle,
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
            button: stylesheet.resolve(&StyleTarget::new("button")),
            hovered_button: stylesheet
                .resolve(&StyleTarget::new("button").with_state(PseudoClass::Hover)),
            pressed_button: stylesheet
                .resolve(&StyleTarget::new("button").with_state(PseudoClass::Pressed)),
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

    fn button_background(&self) -> Color {
        self.button
            .background
            .unwrap_or_else(|| self.fallback.button_background())
    }

    fn button_pressed_background(&self) -> Color {
        self.pressed_button.background.unwrap_or_else(|| {
            self.button
                .background
                .unwrap_or_else(|| self.fallback.button_pressed_background())
        })
    }

    fn button_hover_background(&self) -> Color {
        self.hovered_button.background.unwrap_or_else(|| {
            self.button
                .background
                .unwrap_or_else(|| self.fallback.button_hover_background())
        })
    }

    fn button_padding(&self) -> Insets {
        self.button
            .padding
            .unwrap_or_else(|| self.fallback.button_padding())
    }

    fn spacing(&self) -> f32 {
        self.root.spacing.unwrap_or_else(|| self.fallback.spacing())
    }

    fn font_size(&self) -> f32 {
        self.root
            .font_size
            .unwrap_or_else(|| self.fallback.font_size())
    }

    fn corner_radius(&self) -> f32 {
        self.button
            .corner_radius
            .or(self.root.corner_radius)
            .unwrap_or_else(|| self.fallback.corner_radius())
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
            rules.extend(
                selectors
                    .into_iter()
                    .map(|selector| StyleRule { selector, style }),
            );
            self.skip_ignored()?;
        }
        Ok(StyleSheet { rules })
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
    while cursor < source.len() {
        let character = source[cursor..]
            .chars()
            .next()
            .expect("cursor is within selector source");
        match character {
            '*' if cursor == 0 => cursor += 1,
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
            _ if is_identifier_character(character) && selector.type_name.is_none() => {
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
        "font-size" => style.font_size = Some(parse_length(value, position)?),
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
    let parts: Vec<&str> = value.split_ascii_whitespace().collect();
    let [width, line_style, color] = parts.as_slice() else {
        return Err(TssError {
            position,
            message: "border must be `<width> solid <color>`".to_owned(),
        });
    };
    if *line_style != "solid" {
        return Err(TssError {
            position,
            message: "only `solid` borders are supported".to_owned(),
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
    use crate::{LightTheme, Theme};

    use super::{Color, PseudoClass, StyleSheet, StyleTarget};

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
        assert_eq!(
            style.padding,
            Some(torn_core::Insets::new(4.0, 8.0, 4.0, 8.0))
        );
        assert_eq!(style.font_size, Some(13.0));
    }

    #[test]
    fn resolves_root_and_button_states_into_a_theme() {
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

        assert_eq!(theme.background(), Color::rgba8(16, 24, 32, 255));
        assert_eq!(theme.foreground(), Color::rgba8(241, 243, 245, 255));
        assert_eq!(theme.accent(), Color::rgba8(46, 220, 179, 255));
        assert!((theme.spacing() - 12.0).abs() < f32::EPSILON);
        assert!((theme.font_size() - 15.0).abs() < f32::EPSILON);
        assert!((theme.corner_radius() - 6.0).abs() < f32::EPSILON);
        assert_eq!(theme.button_background(), Color::rgba8(36, 52, 71, 255));
        assert_eq!(
            theme.button_hover_background(),
            Color::rgba8(52, 81, 107, 255)
        );
        assert_eq!(
            theme.button_pressed_background(),
            Color::rgba8(24, 40, 56, 255)
        );
        assert_eq!(
            theme.button_padding(),
            torn_core::Insets::new(6.0, 10.0, 6.0, 10.0)
        );
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
    fn rejects_unknown_properties_and_invalid_values() {
        let error =
            StyleSheet::parse("button { width: 24px; }").expect_err("property is unsupported");
        assert!(error.message().contains("unsupported property `width`"));

        let error = StyleSheet::parse("button { padding: -1px; }").expect_err("negative padding");
        assert!(error.message().contains("non-negative"));
    }
}
