//! A client-side decorated window with a fully custom title bar.
//!
//! Run with `cargo run -p torn --example custom_titlebar`. The example disables
//! native decorations and draws its own chrome, then translates widget window
//! requests into platform actions. It targets Windows and Linux through winit;
//! on macOS the native traffic-light conventions differ, so treat this as a
//! cross-platform starting point rather than a pixel-perfect macOS title bar.

use torn::{
    Badge, BadgeTone, Border, Box as TornBox, Card, Color, Column, Constraints, Divider, Insets,
    Padding, Row, Size, SizedBox, Subscription, Switch, Text, TitleBar, TitleBarStyle, UiRuntime,
    WidgetId, WindowChromeState, WindowControl, WindowFrame,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions, action_for},
    render::{DisplayList, PaintContext, TextStyle},
};

const WINDOW_WIDTH: f32 = 960.0;
const WINDOW_HEIGHT: f32 = 640.0;
const MIN_WIDTH: f32 = 640.0;
const MIN_HEIGHT: f32 = 420.0;
const TITLE_BAR_HEIGHT: f32 = 44.0;

const CHROME: Color = Color::rgba(0.094, 0.102, 0.125, 1.0);
const CANVAS: Color = Color::rgba(0.071, 0.078, 0.102, 1.0);
const PANEL: Color = Color::rgba(0.118, 0.133, 0.173, 1.0);
const PANEL_BORDER: Color = Color::rgba(0.196, 0.216, 0.267, 1.0);
const FOREGROUND: Color = Color::rgba(0.925, 0.937, 0.969, 1.0);
const MUTED: Color = Color::rgba(0.549, 0.580, 0.647, 1.0);
const ACCENT: Color = Color::rgba(0.365, 0.616, 0.980, 1.0);
const CLOSE: Color = Color::rgba(0.863, 0.235, 0.275, 1.0);

fn main() -> Result<(), torn_platform_winit::RunError> {
    torn_platform_winit::run(CustomTitleBar::new())
}

struct CustomTitleBar {
    runtime: UiRuntime,
    size: Size,
    chrome: WindowChromeState,
    _subscriptions: Vec<Subscription>,
}

impl CustomTitleBar {
    fn new() -> Self {
        let size = Size::new(WINDOW_WIDTH, WINDOW_HEIGHT).expect("initial window size is valid");
        let chrome = WindowChromeState::new();

        let title_bar = TitleBar::new("Torn Studio")
            .with_icon("◆")
            .with_subtitle("  custom window chrome")
            .with_controls([
                WindowControl::Minimize,
                WindowControl::Maximize,
                WindowControl::ToggleFullscreen,
                WindowControl::Close,
            ])
            .with_state(chrome.clone())
            .with_style(title_bar_style());

        let frame = WindowFrame::new()
            .with_title_bar_height(TITLE_BAR_HEIGHT)
            .with_resize_border(7.0)
            .with_background(CANVAS)
            .with_border(Border::new(1.0, PANEL_BORDER));

        let mut runtime = UiRuntime::new(frame);
        let root = runtime.root();
        runtime
            .append_child(root, title_bar)
            .expect("window frame exists");

        let content = TornBox::new()
            .with_background(CANVAS)
            .with_padding(Insets::all(32.0));
        let content = runtime
            .append_child(root, content)
            .expect("window frame exists");

        let mut subscriptions = Vec::new();
        build_content(&mut runtime, content, &mut subscriptions);

        let mut app = Self {
            runtime,
            size,
            chrome,
            _subscriptions: subscriptions,
        };
        app.layout(size);
        app
    }

    fn layout(&mut self, size: Size) {
        self.size = size;
        if let Ok(constraints) = Constraints::tight(size) {
            let _ = self.runtime.layout(constraints);
        }
    }
}

fn title_bar_style() -> TitleBarStyle {
    TitleBarStyle {
        height: TITLE_BAR_HEIGHT,
        padding: Insets::new(0.0, 8.0, 0.0, 16.0),
        button_width: 44.0,
        icon_gap: 10.0,
        background: Some(CHROME),
        foreground: Some(FOREGROUND),
        muted_foreground: Some(MUTED),
        hover_background: Some(Color::rgba(0.176, 0.192, 0.235, 1.0)),
        pressed_background: Some(Color::rgba(0.235, 0.255, 0.310, 1.0)),
        close_hover_background: Some(CLOSE),
        close_pressed_background: Some(Color::rgba(0.706, 0.157, 0.196, 1.0)),
        border: Some(Border::new(1.0, PANEL_BORDER)),
        ..TitleBarStyle::default()
    }
}

fn build_content(
    runtime: &mut UiRuntime,
    content: WidgetId,
    subscriptions: &mut Vec<Subscription>,
) {
    let column = runtime
        .append_child(content, Column::new())
        .expect("content box exists");

    append_text(
        runtime,
        column,
        "Client-side window chrome",
        28.0,
        FOREGROUND,
    );
    append_gap(runtime, column, 10.0);
    append_text(
        runtime,
        column,
        "Drag the title bar to move the window. Double-click it to maximize, or use \
         the controls on the right. Press an edge to resize.",
        14.0,
        MUTED,
    );
    append_gap(runtime, column, 22.0);
    build_badges(runtime, column);
    append_gap(runtime, column, 22.0);
    runtime
        .append_child(column, Divider::new().with_color(PANEL_BORDER))
        .expect("content column exists");
    append_gap(runtime, column, 22.0);
    build_card(runtime, column, subscriptions);
    append_gap(runtime, column, 22.0);
    append_text(
        runtime,
        column,
        "Tip: WindowFrame resizes from all eight edges and corners.",
        12.0,
        MUTED,
    );
}

fn build_badges(runtime: &mut UiRuntime, parent: WidgetId) {
    let badges = runtime
        .append_child(parent, Row::new())
        .expect("content column exists");
    for (label, tone, color) in [
        ("Windows", BadgeTone::Accent, Color::WHITE),
        ("Linux", BadgeTone::Success, Color::WHITE),
        ("macOS", BadgeTone::Warning, Color::BLACK),
    ] {
        let badge = runtime
            .append_child(badges, Badge::new().with_tone(tone))
            .expect("badge row exists");
        append_text(runtime, badge, label, 12.0, color);
        runtime
            .append_child(badges, SizedBox::new(Some(8.0), None))
            .expect("badge row exists");
    }
}

fn build_card(runtime: &mut UiRuntime, parent: WidgetId, subscriptions: &mut Vec<Subscription>) {
    let card = Card::new()
        .with_background(PANEL)
        .with_border(Border::new(1.0, PANEL_BORDER))
        .with_corner_radius(14.0)
        .with_elevation(12.0)
        .with_padding(Insets::all(22.0));
    let card = runtime
        .append_child(parent, card)
        .expect("content column exists");
    let column = runtime
        .append_child(card, Column::new())
        .expect("card exists");

    append_text(runtime, column, "Window behaviour", 18.0, FOREGROUND);
    append_gap(runtime, column, 6.0);
    append_text(
        runtime,
        column,
        "Every control below is an ordinary Torn widget. The title bar and \
         frame emit window intents instead of calling a native API directly.",
        13.0,
        MUTED,
    );
    append_gap(runtime, column, 14.0);
    append_switch_row(
        runtime,
        column,
        Switch::with_checked(true)
            .with_on_color(ACCENT)
            .with_off_color(Color::rgba(0.235, 0.255, 0.310, 1.0)),
        "Animated hover states",
        subscriptions,
    );
    append_switch_row(
        runtime,
        column,
        Switch::with_checked(false)
            .with_on_color(ACCENT)
            .with_off_color(Color::rgba(0.235, 0.255, 0.310, 1.0)),
        "Compact title bar",
        subscriptions,
    );
}

fn append_text(
    runtime: &mut UiRuntime,
    parent: WidgetId,
    text: impl Into<String>,
    size: f32,
    color: Color,
) {
    runtime
        .append_child(parent, Text::styled(text, TextStyle::new(size, color)))
        .expect("parent exists");
}

fn append_gap(runtime: &mut UiRuntime, parent: WidgetId, height: f32) {
    runtime
        .append_child(parent, SizedBox::new(None, Some(height)))
        .expect("parent exists");
}

fn append_switch_row(
    runtime: &mut UiRuntime,
    parent: WidgetId,
    switch: Switch,
    label: &str,
    subscriptions: &mut Vec<Subscription>,
) {
    subscriptions.push(switch.toggled().subscribe({
        let label = label.to_owned();
        move |enabled| println!("{label}: {enabled}")
    }));

    let padding = runtime
        .append_child(parent, Padding::new(Insets::new(4.0, 0.0, 4.0, 0.0)))
        .expect("card column exists");
    let row = runtime
        .append_child(padding, Row::new())
        .expect("padding exists");
    runtime
        .append_child(row, switch)
        .expect("switch row exists");
    runtime
        .append_child(row, SizedBox::new(Some(12.0), None))
        .expect("switch row exists");
    runtime
        .append_child(row, Text::styled(label, TextStyle::new(14.0, FOREGROUND)))
        .expect("switch row exists");
}

impl WindowApplication for CustomTitleBar {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Torn — Custom Title Bar", self.size)
            .with_decorations(false)
            .with_resizable(true)
            .with_min_size(Size::new(MIN_WIDTH, MIN_HEIGHT).expect("minimum window size is valid"))
    }

    fn window_event(&mut self, event: WindowEvent) -> WindowAction {
        match event {
            WindowEvent::CloseRequested => WindowAction::Exit,
            WindowEvent::Resized(size) => {
                self.layout(size);
                WindowAction::RequestRedraw
            }
            WindowEvent::Maximized(maximized) => {
                self.chrome.set_maximized(maximized);
                WindowAction::RequestRedraw
            }
            WindowEvent::Fullscreen(fullscreen) => {
                self.chrome.set_fullscreen(fullscreen);
                WindowAction::RequestRedraw
            }
            WindowEvent::Input(event) => {
                let _ = self.runtime.dispatch_event(&event);
                let mut action = WindowAction::None;
                for command in self.runtime.take_window_commands() {
                    action = action_for(command);
                }
                if action == WindowAction::None && self.runtime.take_redraw_request() {
                    action = WindowAction::RequestRedraw;
                }
                action
            }
            WindowEvent::RedrawRequested => WindowAction::None,
        }
    }

    fn redraw(&mut self) -> DisplayList {
        let mut display_list = DisplayList::new();
        let _ = self
            .runtime
            .paint(&mut PaintContext::new(&mut display_list));
        display_list
    }
}
