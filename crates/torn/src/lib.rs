//! Public facade for the Torn UI toolkit.
//!
//! Applications normally depend on this crate alone. Lower-level rendering and
//! headless test support remain available under [`render`] and [`software`].

pub use torn_core::{
    Affine, Color, ConstraintError, Constraints, Diagnostic, DiagnosticReporter,
    DiagnosticSeverity, FocusChanged, InputEvent, Insets, Key, KeyCode, KeyEvent, Modifiers,
    NamedKey, PanicOnDiagnostic, Point, PointerButton, PointerButtons, PointerEvent, PointerId,
    Rect, Size, SizeError, WheelDelta, WheelEvent, WidgetId,
};
pub use torn_ui::{
    Border, ChildLayout, Column, DarkTheme, DirtyFlags, EventContext, EventPhase, EventStatus,
    KeyChord, KeyboardCommand, LayoutContext, LayoutResult, LightTheme, Row, Signal, SurfaceStyle,
    SystemAppearance, SystemTheme, Theme, UiEnvironment, UiRuntime, UiRuntimeError, Widget,
};
pub use torn_widgets::{
    Align, Alignment, Box, Button, ButtonStyle, Container, DockArea, DockAreaError, DockItem,
    ListView, Padding, ScrollAxis, ScrollView, SizedBox, SplitAxis, Splitter, Stack, Text,
};
pub use torn_workspace::{
    DockAxis, DocumentId, DocumentPresentation, FloatingDocument, FloatingPanel, LayoutNode,
    PanelId, SplitPlacement, WorkspaceBounds, WorkspaceError, WorkspaceLayout,
    WorkspaceSerializationError,
};

/// Platform-independent native-window contracts.
pub mod platform {
    pub use torn_platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions};
}

/// Backend-neutral display-list and text-shaping contracts.
pub mod render {
    pub use torn_render::{
        DisplayCommand, DisplayList, FontError, FontFace, FontdueTextShaper, GlyphBitmap, GlyphRun,
        LineMetrics, PaintContext, PositionedGlyph, TextLayout, TextShaper, TextStyle,
    };
}

/// Deterministic headless software rendering and image-test support.
pub mod software {
    pub use torn_software::{
        Pixel, PixelBuffer, PixelBufferError, PngError, ReceiveError, RenderError,
        SoftwareRenderError, SoftwareRenderResult, SoftwareRenderWorker, SoftwareRenderer,
        SubmitError,
    };
}

#[cfg(test)]
mod tests {
    use super::{Box, Button, Color, Constraints, Size, Text, UiRuntime, render};
    use render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle};

    #[test]
    fn exposes_a_complete_headless_widget_pipeline() {
        let mut root = Box::new();
        root.set_background(Some(Color::WHITE));
        let mut runtime = UiRuntime::new(root);
        let button = runtime
            .append_child(runtime.root(), Button::new())
            .expect("root exists");
        let label = FontdueTextShaper::ubuntu_light().layout(
            "Torn",
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );
        runtime
            .append_child(button, Text::new(label.clone()))
            .expect("button exists");
        let mut display_list = DisplayList::new();

        assert_eq!(
            runtime
                .layout(Constraints::UNBOUNDED)
                .expect("widget tree does not panic")
                .size(),
            Size::new(label.size().width() + 16.0, label.size().height() + 16.0)
                .expect("button adds finite padding")
        );
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("widget tree does not panic");

        assert_eq!(display_list.len(), 3);
    }
}
