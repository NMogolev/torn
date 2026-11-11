/// An unpremultiplied sRGBA color with normalized components.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    /// Red component.
    pub red: f32,
    /// Green component.
    pub green: f32,
    /// Blue component.
    pub blue: f32,
    /// Alpha component.
    pub alpha: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    /// Opaque black.
    pub const BLACK: Self = Self::rgba(0.0, 0.0, 0.0, 1.0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgba(1.0, 1.0, 1.0, 1.0);

    /// Creates a color from normalized sRGBA components.
    #[must_use]
    pub const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Creates an opaque color from normalized sRGB components.
    #[must_use]
    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self::rgba(red, green, blue, 1.0)
    }

    /// Creates a color from 8-bit sRGBA components.
    #[must_use]
    pub fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        const SCALE: f32 = 1.0 / 255.0;
        Self::rgba(
            f32::from(red) * SCALE,
            f32::from(green) * SCALE,
            f32::from(blue) * SCALE,
            f32::from(alpha) * SCALE,
        )
    }

    /// Returns this color with a different alpha component.
    #[must_use]
    pub const fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Linearly interpolates from `self` to `other` in sRGB space.
    ///
    /// `amount` is clamped to `0.0..=1.0`; non-finite values are treated as
    /// zero. Themes use this to derive hover and border tints from a small
    /// semantic palette instead of hard-coding every interaction color.
    #[must_use]
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let amount = if amount.is_finite() {
            amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self::rgba(
            self.red + (other.red - self.red) * amount,
            self.green + (other.green - self.green) * amount,
            self.blue + (other.blue - self.blue) * amount,
            self.alpha + (other.alpha - self.alpha) * amount,
        )
    }

    /// Returns this color moved toward white by `amount`.
    #[must_use]
    pub fn lighten(self, amount: f32) -> Self {
        self.mix(Self::WHITE, amount)
    }

    /// Returns this color moved toward black by `amount`.
    #[must_use]
    pub fn darken(self, amount: f32) -> Self {
        self.mix(Self::BLACK, amount)
    }

    /// Returns this color with RGB components premultiplied by alpha.
    ///
    /// Torn's public color values are unpremultiplied. Renderers that use
    /// premultiplied-alpha blending should convert at their boundary.
    #[must_use]
    pub fn into_premultiplied(self) -> Self {
        Self::rgba(
            self.red * self.alpha,
            self.green * self.alpha,
            self.blue * self.alpha,
            self.alpha,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn converts_8_bit_components_to_normalized_values() {
        assert_eq!(
            Color::rgba8(255, 128, 0, 64),
            Color::rgba(1.0, 128.0 / 255.0, 0.0, 64.0 / 255.0)
        );
    }

    #[test]
    fn mixes_and_shifts_colors() {
        let dark = Color::rgba(0.0, 0.0, 0.0, 1.0);
        let light = Color::rgba(1.0, 1.0, 1.0, 1.0);
        assert_eq!(dark.mix(light, 0.5), Color::rgba(0.5, 0.5, 0.5, 1.0));
        assert_eq!(dark.lighten(1.0), Color::WHITE);
        assert_eq!(light.darken(1.0), Color::BLACK);
        assert_eq!(dark.mix(light, 2.0), Color::WHITE);
        assert_eq!(dark.mix(light, f32::NAN), Color::BLACK);
    }

    #[test]
    fn premultiplies_rgb_components_but_preserves_alpha() {
        assert_eq!(
            Color::rgba(0.5, 0.25, 1.0, 0.5).into_premultiplied(),
            Color::rgba(0.25, 0.125, 0.5, 0.5)
        );
    }
}
