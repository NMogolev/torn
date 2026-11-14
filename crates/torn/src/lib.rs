//! Public facade for the Torn UI toolkit.
//!
//! Applications normally depend on this crate alone. Lower-level rendering and
//! headless test support remain available under [`render`] and [`software`].

pub use torn_core::{
    Affine, Clipboard, Color, ConstraintError, Constraints, Diagnostic, DiagnosticReporter,
    DiagnosticSeverity, FocusChanged, InputEvent, Insets, Key, KeyCode, KeyEvent, MemoryClipboard,
    Modifiers, NamedKey, NoopClipboard, PanicOnDiagnostic, Point, PointerButton, PointerButtons,
    PointerEvent, PointerId, Rect, Size, SizeError, TextPreedit, WheelDelta, WheelEvent, WidgetId,
};
#[cfg(feature = "director")]
pub use torn_director::{
    AppLifecycle, DirectorAction, MediaDirector, PlaybackAction, VisibilityPolicy,
};
pub use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, AccessibilityTextSelection, Border, ChildLayout, Column, DEFAULT_DPI,
    DarkTheme, DirtyFlags, EventContext, EventPhase, EventStatus, Font, FontDatabase,
    FontDescriptor, FontFamily, FontMetrics, FontSize, FontStyle, FontWeight, FrameDiagnostics,
    KeyChord, KeyboardCommand, LayoutContext, LayoutResult, LightTheme, PseudoClass, ResizeEdge,
    Row, RuntimeStats, Selector, Signal, StyleRule, StyleSheet, StyleTarget, Styled, Subscription,
    SurfaceStyle, SystemAppearance, SystemTheme, Theme, TssError, TssStyle, TssTheme,
    UiEnvironment, UiRuntime, UiRuntimeError, Widget, WindowCommand,
};
#[cfg(feature = "audio")]
pub use torn_widgets::AudioPaleoSession;
pub use torn_widgets::{
    Alert, AlertTone, Align, Alignment, AnimatedImage, AnimationFrame, Avatar, AvatarStatus, Badge,
    BadgeTone, Box, Button, ButtonStyle, Card, CardStyle, Checkbox, ComboBox, Container, Divider,
    DividerAxis, DockArea, DockAreaError, DockItem, Dropdown, Expander, FfmpegConfig,
    GifDecodeError, Image, ImageFit, ImageViewer, ListView, MediaClock, MediaError, MediaFrame,
    MediaFrameScheduler, MediaMetadata, MediaSession, MediaState, MediaUpdate, MenuBar, Padding,
    ProgressBar, RadioButton, RadioGroup, ScrollAxis, ScrollView, SegmentedControl, SelectionMode,
    SelectionModel, SizedBox, Skeleton, Slider, Spinner, SplitAxis, Splitter, Stack, Switch,
    TabBar, Text, TextEdit, TextField, TextInput, TitleBar, TitleBarStyle, Toolbar, Video,
    VideoError, VideoMetadata, VirtualDataSource, VirtualList, VirtualTable, VirtualTableSource,
    WindowChromeState, WindowControl, WindowFrame,
};
pub use torn_workspace::{
    DockAxis, DocumentId, DocumentPresentation, FloatingDocument, FloatingPanel, LayoutNode,
    MIN_VISIBLE_FLOATING_EXTENT, PanelId, SplitPlacement, WorkspaceBounds, WorkspaceError,
    WorkspaceLayout, WorkspaceSerializationError,
};

/// Platform-independent native-window contracts.
pub mod platform {
    pub use torn_platform::{
        WindowAction, WindowApplication, WindowEvent, WindowOptions, WindowResizeDirection,
    };

    use torn_ui::{ResizeEdge, WindowCommand};

    /// Translates a widget window request into a native-window action.
    ///
    /// Host applications normally call this after draining
    /// [`torn_ui::UiRuntime::take_window_commands`].
    #[must_use]
    pub fn action_for(command: WindowCommand) -> WindowAction {
        match command {
            WindowCommand::StartDrag => WindowAction::StartDrag,
            WindowCommand::Minimize => WindowAction::Minimize,
            WindowCommand::ToggleMaximize => WindowAction::ToggleMaximize,
            WindowCommand::ToggleFullscreen => WindowAction::ToggleFullscreen,
            WindowCommand::Close => WindowAction::Exit,
            WindowCommand::StartResize(edge) => WindowAction::StartResize(match edge {
                ResizeEdge::Top => WindowResizeDirection::North,
                ResizeEdge::Bottom => WindowResizeDirection::South,
                ResizeEdge::Left => WindowResizeDirection::West,
                ResizeEdge::Right => WindowResizeDirection::East,
                ResizeEdge::TopLeft => WindowResizeDirection::NorthWest,
                ResizeEdge::TopRight => WindowResizeDirection::NorthEast,
                ResizeEdge::BottomLeft => WindowResizeDirection::SouthWest,
                ResizeEdge::BottomRight => WindowResizeDirection::SouthEast,
            }),
        }
    }
}

/// Backend-neutral display-list and text-shaping contracts.
pub mod render {
    pub use torn_render::{
        DisplayCommand, DisplayList, DisplayListStats, FontError, FontFace, FontdueTextShaper,
        GlyphBitmap, GlyphRun, LineMetrics, PaintContext, PaintTransition, PositionedGlyph,
        RasterImage, RasterImageError, TextLayout, TextLayoutCacheStats, TextShaper, TextStyle,
        TextureId,
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

/// GPU display-list rendering powered by `wgpu`.
pub mod gpu {
    pub use torn_wgpu::{GpuError, GpuRenderer, GpuRendererError, RenderStatus};
}

/// Servo browser surfaces and their Torn widget.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub mod browser {
    pub use torn_browser::{
        BrowserError, BrowserSurface, BrowserView, Context, InteropError, SharedTexture,
    };
}

#[cfg(test)]
mod tests {
    use super::{
        Box, Button, Color, Constraints, LightTheme, Size, Text, Theme, UiRuntime, render,
    };
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
        let padding = LightTheme.control_padding();
        runtime
            .append_child(button, Text::new(label.clone()))
            .expect("button exists");
        let mut display_list = DisplayList::new();

        assert_eq!(
            runtime
                .layout(Constraints::UNBOUNDED)
                .expect("widget tree does not panic")
                .size(),
            Size::new(
                label.size().width() + padding.left + padding.right,
                label.size().height() + padding.top + padding.bottom,
            )
            .expect("button adds finite padding")
        );
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("widget tree does not panic");

        assert_eq!(display_list.len(), 3);
    }
}
