use core::fmt;

use crate::Size;

/// A valid range of sizes a widget may occupy during layout.
///
/// Every instance preserves `0 <= min <= max` independently for width and
/// height. Maximum dimensions may be positive infinity to represent an
/// unbounded axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    min: Size,
    max: Size,
}

impl Constraints {
    /// Constraints that permit only zero size.
    pub const ZERO: Self = Self {
        min: Size::ZERO,
        max: Size::ZERO,
    };

    /// Constraints without upper bounds.
    pub const UNBOUNDED: Self = Self {
        min: Size::ZERO,
        max: Size::UNBOUNDED,
    };

    /// Builds validated constraints.
    ///
    /// # Errors
    ///
    /// Returns [`ConstraintError`] when a minimum is infinite or a minimum
    /// exceeds its maximum. [`Size`] rejects negative and NaN dimensions when
    /// it is constructed.
    pub fn new(min: Size, max: Size) -> Result<Self, ConstraintError> {
        if !min.width().is_finite() || !min.height().is_finite() {
            return Err(ConstraintError::InvalidMinimum);
        }

        if min.width() > max.width() || min.height() > max.height() {
            return Err(ConstraintError::MinimumExceedsMaximum);
        }

        Ok(Self { min, max })
    }

    /// Builds constraints whose lower bound is zero.
    #[must_use]
    pub const fn loose(max: Size) -> Self {
        Self {
            min: Size::ZERO,
            max,
        }
    }

    /// Builds constraints that require exactly `size`.
    ///
    /// # Errors
    ///
    /// Returns [`ConstraintError`] when `size` has an infinite dimension.
    pub fn tight(size: Size) -> Result<Self, ConstraintError> {
        Self::new(size, size)
    }

    /// Returns the minimum permitted size.
    #[must_use]
    pub const fn min(self) -> Size {
        self.min
    }

    /// Returns the maximum permitted size.
    #[must_use]
    pub const fn max(self) -> Size {
        self.max
    }

    /// Clamps `size` into this range.
    #[must_use]
    pub fn constrain(self, size: Size) -> Size {
        size.clamp(self.min, self.max)
    }

    /// Returns whether `size` lies within this range.
    #[must_use]
    pub fn contains(self, size: Size) -> bool {
        size.width() >= self.min.width()
            && size.width() <= self.max.width()
            && size.height() >= self.min.height()
            && size.height() <= self.max.height()
    }
}

/// Why a [`Constraints`] constructor rejected its bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintError {
    /// A minimum dimension was infinite.
    InvalidMinimum,
    /// A minimum dimension was greater than its corresponding maximum.
    MinimumExceedsMaximum,
}

impl fmt::Display for ConstraintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMinimum => "constraint minima must be finite",
            Self::MinimumExceedsMaximum => "constraint minimum cannot exceed maximum",
        })
    }
}

impl std::error::Error for ConstraintError {}

#[cfg(test)]
mod tests {
    use super::{ConstraintError, Constraints};
    use crate::Size;

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn accepts_unbounded_axes_and_preserves_bounds() {
        let constraints = Constraints::new(size(10.0, 20.0), size(f32::INFINITY, 100.0))
            .expect("valid constraints");

        assert_eq!(constraints.min(), size(10.0, 20.0));
        assert_eq!(constraints.max(), size(f32::INFINITY, 100.0));
        assert_eq!(constraints.constrain(size(5.0, 120.0)), size(10.0, 100.0));
    }

    #[test]
    fn rejects_infinite_or_reversed_minima() {
        assert_eq!(
            Constraints::new(size(f32::INFINITY, 0.0), size(f32::INFINITY, 1.0)),
            Err(ConstraintError::InvalidMinimum)
        );
        assert_eq!(
            Constraints::new(size(2.0, 0.0), size(1.0, 1.0)),
            Err(ConstraintError::MinimumExceedsMaximum)
        );
    }
}
