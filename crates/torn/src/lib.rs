//! Public facade for the Torn UI toolkit.
//!
//! Applications normally depend on this crate alone. Lower-level rendering and
//! headless test support remain available under [`render`] and [`software`].

pub use torn_core::{
    Color, ConstraintError, Constraints, Diagnostic, DiagnosticReporter, DiagnosticSeverity,
    FocusChanged, InputEvent, Insets, Key, KeyCode, KeyEvent, Modifiers, NamedKey,
    PanicOnDiagnostic, Point, PointerButton, PointerButtons, PointerEvent, PointerId, Rect, Size,
    SizeError, WheelDelta, WheelEvent,
};
pub use torn_ui::{
    ChildLayout, Column, EventStatus, LayoutResult, Row, UiRuntime, UiRuntimeError, Widget,
};
pub use torn_widgets::{Box, Button, Text};

/// Backend-neutral display-list and text-shaping contracts.
pub mod render {
    pub use torn_render::{
        DisplayCommand, DisplayList, PaintContext, TextLayout, TextShaper, TextStyle,
    };
}

/// Deterministic headless software rendering and image-test support.
pub mod software {
    pub use torn_software::{
        Pixel, PixelBuffer, PixelBufferError, PngError, RenderError, SoftwareRenderer,
    };
}

#[cfg(test)]
mod tests {
    use super::{Box, Button, Color, Constraints, Size, Text, UiRuntime, render};
    use render::{DisplayList, PaintContext, TextLayout};

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn exposes_a_complete_headless_widget_pipeline() {
        let label = Text::new(TextLayout::new(size(40.0, 16.0), Color::BLACK));
        let mut root = Box::with_child(Button::new(label));
        root.set_background(Some(Color::WHITE));
        let mut runtime = UiRuntime::new(root);
        let mut display_list = DisplayList::new();

        assert_eq!(
            runtime
                .layout(Constraints::UNBOUNDED)
                .expect("widget tree does not panic")
                .size(),
            size(56.0, 32.0)
        );
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("widget tree does not panic");

        assert_eq!(display_list.len(), 3);
    }
}
