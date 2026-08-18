/// A two-dimensional position in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
}

impl Point {
    /// The coordinate origin.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// Creates a point at the supplied logical-pixel coordinates.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A two-dimensional affine transformation in logical-pixel coordinates.
///
/// The transform maps a point `(x, y)` to
/// `(xx * x + yx * y + dx, xy * x + yy * y + dy)`. Transforms are composed in
/// drawing order: `first.then(second)` applies `first` and then `second`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// Horizontal contribution to the output x coordinate.
    pub xx: f32,
    /// Vertical contribution to the output x coordinate.
    pub yx: f32,
    /// Horizontal contribution to the output y coordinate.
    pub xy: f32,
    /// Vertical contribution to the output y coordinate.
    pub yy: f32,
    /// Horizontal translation.
    pub dx: f32,
    /// Vertical translation.
    pub dy: f32,
}

impl Affine {
    /// The identity transform.
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    /// Creates a transform from its six matrix components.
    #[must_use]
    pub const fn new(xx: f32, yx: f32, xy: f32, yy: f32, dx: f32, dy: f32) -> Self {
        Self {
            xx,
            yx,
            xy,
            yy,
            dx,
            dy,
        }
    }

    /// Creates a translation transform.
    #[must_use]
    pub const fn translate(x: f32, y: f32) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// Creates a scale transform around the origin.
    #[must_use]
    pub const fn scale(x: f32, y: f32) -> Self {
        Self::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    /// Creates a counter-clockwise rotation transform around the origin.
    #[must_use]
    pub fn rotate(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(cos, -sin, sin, cos, 0.0, 0.0)
    }

    /// Returns whether every matrix component is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.xx.is_finite()
            && self.yx.is_finite()
            && self.xy.is_finite()
            && self.yy.is_finite()
            && self.dx.is_finite()
            && self.dy.is_finite()
    }

    /// Applies the transform to `point`.
    #[must_use]
    pub const fn transform_point(self, point: Point) -> Point {
        Point::new(
            self.xx * point.x + self.yx * point.y + self.dx,
            self.xy * point.x + self.yy * point.y + self.dy,
        )
    }

    /// Returns the transform that applies `self` and then `next`.
    #[must_use]
    pub const fn then(self, next: Self) -> Self {
        Self::new(
            next.xx * self.xx + next.yx * self.xy,
            next.xx * self.yx + next.yx * self.yy,
            next.xy * self.xx + next.yy * self.xy,
            next.xy * self.yx + next.yy * self.yy,
            next.xx * self.dx + next.yx * self.dy + next.dx,
            next.xy * self.dx + next.yy * self.dy + next.dy,
        )
    }

    /// Returns the inverse transform, or `None` when the transform is singular
    /// or has non-finite components.
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }
        let determinant = self.xx * self.yy - self.yx * self.xy;
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inverse_determinant = determinant.recip();
        Some(Self::new(
            self.yy * inverse_determinant,
            -self.yx * inverse_determinant,
            -self.xy * inverse_determinant,
            self.xx * inverse_determinant,
            (self.yx * self.dy - self.yy * self.dx) * inverse_determinant,
            (self.xy * self.dx - self.xx * self.dy) * inverse_determinant,
        ))
    }
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}
/// A two-dimensional extent in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Horizontal extent.
    width: f32,
    /// Vertical extent.
    height: f32,
}

impl Size {
    /// A size with no extent.
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub(crate) const UNBOUNDED: Self = Self {
        width: f32::INFINITY,
        height: f32::INFINITY,
    };

    /// Creates a non-negative size with the supplied extents.
    ///
    /// Positive infinity is accepted for an unbounded [`crate::Constraints`]
    /// maximum. A measured widget size must be finite, which is enforced by
    /// [`crate::Constraints`] when used as a minimum or tight size.
    ///
    /// # Errors
    ///
    /// Returns [`SizeError`] when either extent is negative or NaN.
    pub fn new(width: f32, height: f32) -> Result<Self, SizeError> {
        if width.is_nan() || width < 0.0 {
            return Err(SizeError::InvalidWidth);
        }

        if height.is_nan() || height < 0.0 {
            return Err(SizeError::InvalidHeight);
        }

        Ok(Self { width, height })
    }

    /// Returns the horizontal extent.
    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    /// Returns the vertical extent.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    /// Returns this size clamped to the inclusive range between `min` and `max`.
    #[must_use]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            width: self.width.clamp(min.width, max.width),
            height: self.height.clamp(min.height, max.height),
        }
    }
}

/// Why a [`Size`] constructor rejected an extent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SizeError {
    /// The width was negative or NaN.
    InvalidWidth,
    /// The height was negative or NaN.
    InvalidHeight,
}

impl core::fmt::Display for SizeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidWidth => "size width must be non-negative and not NaN",
            Self::InvalidHeight => "size height must be non-negative and not NaN",
        })
    }
}

impl std::error::Error for SizeError {}

/// Insets measured from the four edges of a rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Inset from the top edge.
    pub top: f32,
    /// Inset from the right edge.
    pub right: f32,
    /// Inset from the bottom edge.
    pub bottom: f32,
    /// Inset from the left edge.
    pub left: f32,
}

impl Insets {
    /// No inset on any edge.
    pub const ZERO: Self = Self::all(0.0);

    /// Creates equal insets on every edge.
    #[must_use]
    pub const fn all(value: f32) -> Self {
        Self::new(value, value, value, value)
    }

    /// Creates insets from top, right, bottom, and left values.
    #[must_use]
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Returns the total horizontal inset.
    #[must_use]
    pub const fn horizontal(self) -> f32 {
        self.left + self.right
    }

    /// Returns the total vertical inset.
    #[must_use]
    pub const fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

/// An axis-aligned rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Top-left origin.
    pub origin: Point,
    /// Extent from the origin.
    pub size: Size,
}

impl Rect {
    /// An empty rectangle at the origin.
    pub const ZERO: Self = Self::new(Point::ZERO, Size::ZERO);

    /// Creates a rectangle from an origin and a size.
    #[must_use]
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// Returns the x coordinate immediately past the right edge.
    #[must_use]
    pub const fn right(self) -> f32 {
        self.origin.x + self.size.width()
    }

    /// Returns the y coordinate immediately past the bottom edge.
    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.origin.y + self.size.height()
    }

    /// Returns whether `point` is within this rectangle.
    ///
    /// The left and top edges are inclusive; the right and bottom edges are
    /// exclusive. Empty rectangles contain no points.
    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        self.size.width() > 0.0
            && self.size.height() > 0.0
            && point.x >= self.origin.x
            && point.x < self.right()
            && point.y >= self.origin.y
            && point.y < self.bottom()
    }

    /// Returns this rectangle reduced by `insets`, clamping each extent to zero.
    #[must_use]
    pub fn inset(self, insets: Insets) -> Self {
        Self::new(
            Point::new(self.origin.x + insets.left, self.origin.y + insets.top),
            Size {
                width: (self.size.width() - insets.horizontal()).max(0.0),
                height: (self.size.height() - insets.vertical()).max(0.0),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Affine, Insets, Point, Rect, Size, SizeError};

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn size_rejects_negative_and_nan_extents() {
        assert_eq!(Size::new(-1.0, 0.0), Err(SizeError::InvalidWidth));
        assert_eq!(Size::new(0.0, f32::NAN), Err(SizeError::InvalidHeight));
    }

    #[test]
    fn rectangle_uses_half_open_edges_for_hit_testing() {
        let rect = Rect::new(Point::new(10.0, 20.0), size(30.0, 40.0));

        assert!(rect.contains(Point::new(10.0, 20.0)));
        assert!(rect.contains(Point::new(39.9, 59.9)));
        assert!(!rect.contains(Point::new(40.0, 20.0)));
        assert!(!rect.contains(Point::new(10.0, 60.0)));
        assert!(!Rect::ZERO.contains(Point::ZERO));
    }

    #[test]
    fn insetting_clamps_an_overconstrained_extent_to_zero() {
        let rect = Rect::new(Point::new(5.0, 10.0), size(20.0, 10.0));
        let result = rect.inset(Insets::new(3.0, 12.0, 8.0, 12.0));

        assert_eq!(result.origin, Point::new(17.0, 13.0));
        assert_eq!(result.size, Size::ZERO);
    }

    #[test]
    fn affine_composition_and_inverse_round_trip_a_point() {
        let transform = Affine::scale(2.0, 3.0).then(Affine::translate(5.0, -4.0));
        let point = Point::new(2.0, 7.0);

        assert_eq!(transform.transform_point(point), Point::new(9.0, 17.0));
        let round_trip = transform
            .inverse()
            .expect("non-singular transform has an inverse")
            .transform_point(transform.transform_point(point));
        assert!((round_trip.x - point.x).abs() < 1e-5);
        assert!((round_trip.y - point.y).abs() < 1e-5);
    }
}
