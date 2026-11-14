//! Public facade for the Drawen UI toolkit.
//!
//! Drawen builds native desktop interfaces from a retained widget tree. Widgets
//! measure, lay themselves out, and record backend-neutral paint commands.
//! A software renderer, a `wgpu` renderer, and the native window adapters
//! execute the same commands, so an application can develop and test headless
//! and then present the same tree in a window.
//!
//! Applications normally depend on this crate alone. Lower-level rendering,
//! platform, GPU, browser, and headless test support remain available under
//! [`render`], [`software`], [`gpu`], [`platform`], and [`browser`].
//!
//! # Quick start
//!
//! ```
//! use drawen::{
//!     Box as DrawenBox, Button, Color, Constraints, Size, Text, UiRuntime,
//!     render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
//! };
//!
//! let label = Text::new(FontdueTextShaper::ubuntu_light().layout(
//!     "Hello, Drawen",
//!     &TextStyle::new(16.0, Color::BLACK),
//!     None,
//! ));
//!
//! let mut root = DrawenBox::new();
//! root.set_background(Some(Color::WHITE));
//!
//! let mut runtime = UiRuntime::new(root);
//! let button = runtime
//!     .append_child(runtime.root(), Button::new())
//!     .expect("the root exists");
//! runtime
//!     .append_child(button, label)
//!     .expect("the button exists");
//! runtime
//!     .layout(Constraints::tight(Size::new(320.0, 180.0).expect("valid size")).expect("valid constraints"))
//!     .expect("layout succeeds");
//!
//! let mut display_list = DisplayList::new();
//! runtime
//!     .paint(&mut PaintContext::new(&mut display_list))
//!     .expect("paint succeeds");
//! assert!(!display_list.is_empty());
//! ```
//!
//! # Features
//!
//! - `audio` enables audio-mastered Paleo playback through the platform output
//!   device.
//! - `director` enables `MediaDirector`, which maps window lifecycle events to
//!   playback policy.

pub use drawen_core::{
    Affine, Clipboard, Color, ConstraintError, Constraints, Diagnostic, DiagnosticReporter,
    DiagnosticSeverity, FocusChanged, InputEvent, Insets, Key, KeyCode, KeyEvent, MemoryClipboard,
    Modifiers, NamedKey, NoopClipboard, PanicOnDiagnostic, Point, PointerButton, PointerButtons,
    PointerEvent, PointerId, Rect, Size, SizeError, TextPreedit, WheelDelta, WheelEvent, WidgetId,
};
#[cfg(feature = "director")]
pub use drawen_director::{
    AppLifecycle, DirectorAction, MediaDirector, PlaybackAction, VisibilityPolicy,
};
pub use drawen_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, AccessibilityTextSelection, Border, ChildLayout, Column, DEFAULT_DPI,
    DarkTheme, DirtyFlags, DssError, DssStyle, DssTheme, EventContext, EventPhase, EventStatus,
    Font, FontDatabase, FontDescriptor, FontFamily, FontMetrics, FontSize, FontStyle, FontWeight,
    FrameDiagnostics, KeyChord, KeyboardCommand, LayoutContext, LayoutResult, LightTheme,
    PseudoClass, ResizeEdge, Row, RuntimeStats, Selector, Signal, StyleRule, StyleSheet,
    StyleTarget, Styled, Subscription, SurfaceStyle, SystemAppearance, SystemTheme, Theme,
    UiEnvironment, UiRuntime, UiRuntimeError, Widget, WindowCommand,
};
#[cfg(feature = "audio")]
pub use drawen_widgets::AudioPaleoSession;
pub use drawen_widgets::{
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
pub use drawen_workspace::{
    DockAxis, DocumentId, DocumentPresentation, FloatingDocument, FloatingPanel, LayoutNode,
    MIN_VISIBLE_FLOATING_EXTENT, PanelId, SplitPlacement, WorkspaceBounds, WorkspaceError,
    WorkspaceLayout, WorkspaceSerializationError,
};

/// Platform-independent native-window contracts.
pub mod platform {
    pub use drawen_platform::{
        WindowAction, WindowApplication, WindowEvent, WindowOptions, WindowResizeDirection,
    };

    use drawen_ui::{ResizeEdge, WindowCommand};

    /// Translates a widget window request into a native-window action.
    ///
    /// Host applications normally call this after draining
    /// [`drawen_ui::UiRuntime::take_window_commands`].
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
    pub use drawen_render::{
        DisplayCommand, DisplayList, DisplayListStats, FontError, FontFace, FontdueTextShaper,
        GlyphBitmap, GlyphRun, LineMetrics, PaintContext, PaintTransition, PositionedGlyph,
        RasterImage, RasterImageError, TextLayout, TextLayoutCacheStats, TextShaper, TextStyle,
        TextureId,
    };
}

/// Deterministic headless software rendering and image-test support.
pub mod software {
    pub use drawen_software::{
        Pixel, PixelBuffer, PixelBufferError, PngError, ReceiveError, RenderError,
        SoftwareRenderError, SoftwareRenderResult, SoftwareRenderWorker, SoftwareRenderer,
        SubmitError,
    };
}

/// GPU display-list rendering powered by `wgpu`.
pub mod gpu {
    pub use drawen_wgpu::{GpuError, GpuRenderer, GpuRendererError, RenderStatus};
}

/// Servo browser surfaces and their Drawen widget.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub mod browser {
    pub use drawen_browser::{
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
            "Drawen",
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
