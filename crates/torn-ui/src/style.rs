use torn_core::{Color, Insets};

/// A visible border around a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Border width in logical pixels.
    pub width: f32,
    /// Unpremultiplied sRGBA border color.
    pub color: Color,
}

impl Border {
    /// Creates a border with `width` logical pixels and `color`.
    #[must_use]
    pub const fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

/// Locally overridable visual properties shared by surface-like widgets.
///
/// An absent property is deliberately different from a zero or transparent
/// value: widgets may resolve it from their theme or their own default. Margin
/// is not part of this type because it is interpreted by the parent layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurfaceStyle {
    /// Fill color for the surface.
    pub background: Option<Color>,
    /// Optional border painted over the surface fill.
    pub border: Option<Border>,
    /// Radius of all surface corners in logical pixels.
    pub corner_radius: Option<f32>,
    /// Empty space between the surface edge and its direct child.
    pub padding: Option<Insets>,
}
