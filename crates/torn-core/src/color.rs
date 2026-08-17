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
    fn premultiplies_rgb_components_but_preserves_alpha() {
        assert_eq!(
            Color::rgba(0.5, 0.25, 1.0, 0.5).into_premultiplied(),
            Color::rgba(0.25, 0.125, 0.5, 0.5)
        );
    }
}
