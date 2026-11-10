//! Retained widget tree, layout lifecycle, routing, and invalidation for Torn.

mod accessibility;
mod column;
mod command;
mod environment;
mod event;
mod layout;
mod row;
mod runtime;
mod signal;
mod style;
mod styled;
mod theme;
mod tss;
mod widget;

pub use torn_render::{
    DEFAULT_DPI, Font, FontDatabase, FontDescriptor, FontFamily, FontMetrics, FontSize, FontStyle,
    FontWeight,
};

pub(crate) use accessibility::SemanticsBuilder;
pub use accessibility::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, AccessibilityTextSelection, SemanticNode, SemanticTree,
    accessibility_action_from_accesskit, widget_id_from_accesskit,
};
pub use column::Column;
pub use command::{KeyChord, KeyboardCommand};
pub use environment::UiEnvironment;
pub use event::{EventContext, EventPhase, EventStatus, ResizeEdge, WindowCommand};
pub use layout::{ChildLayout, LayoutResult};
pub use row::Row;
pub use runtime::{
    DirtyFlags, FrameDiagnostics, LayoutContext, RuntimeStats, UiRuntime, UiRuntimeError,
};
pub use signal::{Signal, Subscription};
pub use style::{Border, SurfaceStyle};
pub use styled::Styled;
pub use theme::{DarkTheme, LightTheme, SystemAppearance, SystemTheme, Theme};
pub use tss::{
    PseudoClass, Selector, StyleRule, StyleSheet, StyleTarget, TssError, TssStyle, TssTheme,
};
pub use widget::Widget;
