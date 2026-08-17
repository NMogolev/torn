//! Dependency-free geometry, identity, color, and input types for Torn.
//!
//! This crate intentionally contains only data contracts shared by higher-level
//! Torn crates. It has no platform, rendering, or widget-runtime dependency.

mod color;
mod constraints;
mod diagnostic;
mod geometry;
mod id;
mod input;

pub use color::Color;
pub use constraints::{ConstraintError, Constraints};
pub use diagnostic::{Diagnostic, DiagnosticReporter, DiagnosticSeverity, PanicOnDiagnostic};
pub use geometry::{Insets, Point, Rect, Size, SizeError};
pub use id::WidgetId;
pub use input::{
    FocusChanged, InputEvent, Key, KeyCode, KeyEvent, Modifiers, NamedKey, PointerButton,
    PointerButtons, PointerEvent, PointerId, WheelDelta, WheelEvent,
};
