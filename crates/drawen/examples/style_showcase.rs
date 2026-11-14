//! Interactive component laboratory for all standard Drawen widgets.
//!
//! Run with `cargo run -p drawen --example style_showcase`.
//!
//! The showcase follows the Qt/AUI idea of a directly configurable interface:
//! the "Appearance" page edits the same semantic theme that every standard
//! widget resolves during layout and paint, so palette, typography, density,
//! corner radius, and animation changes apply live.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use drawen::{
    Alert, AlertTone, Avatar, AvatarStatus, Badge, BadgeTone, Button, Checkbox, ChildLayout, Color,
    Constraints, Divider, DockArea, DockAxis, Dropdown, Expander, Image, Insets, LayoutContext,
    LayoutNode, LayoutResult, ListView, MenuBar, PanelId, Point, ProgressBar, RadioButton,
    RadioGroup, Rect, ScrollView, SegmentedControl, Size, Skeleton, Slider, Spinner, SplitAxis,
    Splitter, Subscription, Switch, TabBar, TextEdit, TextField, Theme, Toolbar, UiEnvironment,
    UiRuntime, Widget, WorkspaceLayout,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, PaintContext, RasterImage, TextLayout, TextStyle},
};

const WINDOW_WIDTH: f32 = 1_320.0;
const WINDOW_HEIGHT: f32 = 860.0;
const WINDOW_WIDTH_PX: u32 = 1_320;
const WINDOW_HEIGHT_PX: u32 = 860;
const MENU_MAX_HEIGHT: f32 = 58.0;
const TOOLBAR_MAX_HEIGHT: f32 = 78.0;
const STATUS_HEIGHT: f32 = 28.0;
const PAGE_PADDING: f32 = 24.0;
const BASE_FONT_SIZE: f32 = 14.0;
const BASE_SPACING: f32 = 8.0;

fn main() -> Result<(), drawen_platform_winit::RunError> {
    if std::env::args().any(|argument| argument == "--snapshot") {
        write_snapshots();
        return Ok(());
    }
    drawen_platform_winit::run(StyleShowcase::new(0))
}

/// Renders every page to `target/showcase/*.png` without opening a window.
fn write_snapshots() {
    std::fs::create_dir_all("target/showcase").expect("snapshot directory is writable");
    for (index, name) in [
        "overview",
        "components",
        "appearance",
        "layout",
        "workspace",
    ]
    .iter()
    .enumerate()
    {
        let mut app = StyleShowcase::new(index);
        render_snapshot(&mut app, name);
    }

    let mut light = StyleShowcase::new(2);
    {
        let mut appearance = light.appearance.borrow_mut();
        appearance.preset = Preset::Daylight;
        appearance.accent_hue = 215.0;
        appearance.font_scale = 1.18;
        appearance.spacing_scale = 1.22;
        appearance.radius = 16.0;
        appearance.animations = false;
    }
    light
        .runtime
        .environment_mut()
        .set_animations_enabled(false);
    light.runtime.invalidate_layout();
    render_snapshot(&mut light, "appearance-light");

    let mut components = StyleShowcase::new(1);
    components.layout(valid_size(WINDOW_WIDTH, 3_000.0));
    render_snapshot_sized(&mut components, "components-tall", WINDOW_WIDTH_PX, 3_000);
}

fn render_snapshot(app: &mut StyleShowcase, name: &str) {
    render_snapshot_sized(app, name, WINDOW_WIDTH_PX, WINDOW_HEIGHT_PX);
}

fn render_snapshot_sized(app: &mut StyleShowcase, name: &str, width: u32, height: u32) {
    use drawen::software::{PixelBuffer, SoftwareRenderer};

    let mut display_list = DisplayList::new();
    app.runtime
        .paint(&mut PaintContext::new(&mut display_list))
        .expect("showcase paints");
    let mut pixels = PixelBuffer::new(width, height).expect("snapshot buffer is valid");
    SoftwareRenderer
        .render(&display_list, &mut pixels)
        .expect("showcase renders");
    let path = format!("target/showcase/{name}.png");
    pixels.write_png(&path).expect("snapshot is saved");
    println!("{path}");
}

// ---------------------------------------------------------------------------
// Appearance model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preset {
    Midnight,
    Daylight,
    Graphite,
}

impl Preset {
    const ALL: [Self; 3] = [Self::Midnight, Self::Daylight, Self::Graphite];
    const LABELS: [&'static str; 3] = ["Midnight", "Daylight", "Graphite"];

    const fn label(self) -> &'static str {
        match self {
            Self::Midnight => "Midnight",
            Self::Daylight => "Daylight",
            Self::Graphite => "Graphite",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Midnight => 0,
            Self::Daylight => 1,
            Self::Graphite => 2,
        }
    }

    const fn is_dark(self) -> bool {
        !matches!(self, Self::Daylight)
    }
}

#[derive(Clone, Copy, Debug)]
struct Appearance {
    preset: Preset,
    accent_hue: f32,
    font_scale: f32,
    spacing_scale: f32,
    radius: f32,
    animations: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            preset: Preset::Midnight,
            accent_hue: 168.0,
            font_scale: 1.0,
            spacing_scale: 1.0,
            radius: 10.0,
            animations: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Palette {
    background: Color,
    surface: Color,
    raised: Color,
    control: Color,
    control_hover: Color,
    control_pressed: Color,
    foreground: Color,
    muted: Color,
    border: Color,
    accent: Color,
    accent_foreground: Color,
    title_bar: Color,
    shadow: Color,
}

fn palette_for(appearance: &Appearance) -> Palette {
    let (
        background,
        surface,
        raised,
        control,
        control_hover,
        control_pressed,
        foreground,
        muted,
        border,
        shadow,
    ) = match appearance.preset {
        Preset::Midnight => (
            Color::rgba8(12, 15, 23, 255),
            Color::rgba8(19, 24, 35, 255),
            Color::rgba8(25, 31, 45, 255),
            Color::rgba8(32, 40, 56, 255),
            Color::rgba8(42, 52, 71, 255),
            Color::rgba8(54, 65, 88, 255),
            Color::rgba8(233, 238, 247, 255),
            Color::rgba8(151, 163, 186, 255),
            Color::rgba8(48, 58, 79, 255),
            Color::rgba8(0, 0, 0, 150),
        ),
        Preset::Daylight => (
            Color::rgba8(242, 245, 250, 255),
            Color::rgba8(255, 255, 255, 255),
            Color::rgba8(255, 255, 255, 255),
            Color::rgba8(237, 241, 247, 255),
            Color::rgba8(226, 232, 241, 255),
            Color::rgba8(214, 222, 234, 255),
            Color::rgba8(28, 32, 41, 255),
            Color::rgba8(101, 110, 126, 255),
            Color::rgba8(211, 219, 231, 255),
            Color::rgba8(23, 32, 53, 42),
        ),
        Preset::Graphite => (
            Color::rgba8(28, 29, 32, 255),
            Color::rgba8(36, 37, 41, 255),
            Color::rgba8(44, 45, 50, 255),
            Color::rgba8(50, 52, 58, 255),
            Color::rgba8(60, 62, 69, 255),
            Color::rgba8(72, 74, 83, 255),
            Color::rgba8(241, 242, 245, 255),
            Color::rgba8(166, 169, 178, 255),
            Color::rgba8(61, 63, 70, 255),
            Color::rgba8(0, 0, 0, 135),
        ),
    };

    let accent = accent_color(appearance.preset.is_dark(), appearance.accent_hue);
    Palette {
        background,
        surface,
        raised,
        control,
        control_hover,
        control_pressed,
        foreground,
        muted,
        border,
        accent,
        accent_foreground: contrast_on(accent),
        title_bar: background.mix(surface, 0.5),
        shadow,
    }
}

fn accent_color(dark: bool, hue: f32) -> Color {
    if dark {
        hsl(hue, 0.64, 0.62)
    } else {
        hsl(hue, 0.72, 0.43)
    }
}

fn accent_name(hue: f32) -> &'static str {
    let hue = hue.rem_euclid(360.0);
    if !(20.0..340.0).contains(&hue) {
        "Crimson"
    } else if hue < 45.0 {
        "Amber"
    } else if hue < 70.0 {
        "Lemon"
    } else if hue < 150.0 {
        "Emerald"
    } else if hue < 195.0 {
        "Mint"
    } else if hue < 250.0 {
        "Azure"
    } else if hue < 290.0 {
        "Lilac"
    } else if hue < 320.0 {
        "Purple"
    } else {
        "Rose"
    }
}

fn hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let hue = hue.rem_euclid(360.0);
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let scaled = hue / 60.0;
    let x = chroma * (1.0 - (scaled % 2.0 - 1.0).abs());
    #[allow(clippy::cast_possible_truncation)]
    let sector = scaled.floor() as i32;
    let (red, green, blue) = match sector {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    Color::rgb(red + m, green + m, blue + m)
}

fn contrast_on(color: Color) -> Color {
    let luminance = 0.2126 * color.red + 0.7152 * color.green + 0.0722 * color.blue;
    if luminance > 0.58 {
        Color::rgba8(13, 16, 22, 255)
    } else {
        Color::WHITE
    }
}

/// Semantic theme backed by the shared, live [`Appearance`] state.
struct ShowcaseTheme {
    appearance: Rc<RefCell<Appearance>>,
}

impl ShowcaseTheme {
    fn palette(&self) -> Palette {
        palette_for(&self.appearance.borrow())
    }

    fn appearance(&self) -> Appearance {
        *self.appearance.borrow()
    }
}

impl Theme for ShowcaseTheme {
    fn background(&self) -> Color {
        self.palette().background
    }

    fn surface(&self) -> Color {
        self.palette().surface
    }

    fn raised_surface(&self) -> Color {
        self.palette().raised
    }

    fn foreground(&self) -> Color {
        self.palette().foreground
    }

    fn accent(&self) -> Color {
        self.palette().accent
    }

    fn accent_foreground(&self) -> Color {
        self.palette().accent_foreground
    }

    fn control_background(&self) -> Color {
        self.palette().control
    }

    fn control_hover_background(&self) -> Color {
        self.palette().control_hover
    }

    fn control_pressed_background(&self) -> Color {
        self.palette().control_pressed
    }

    fn border(&self) -> Color {
        self.palette().border
    }

    fn focus_ring(&self) -> Color {
        self.palette().accent
    }

    fn shadow(&self) -> Color {
        self.palette().shadow
    }

    fn muted_foreground(&self) -> Color {
        self.palette().muted
    }

    fn title_bar_background(&self) -> Color {
        self.palette().title_bar
    }

    fn title_bar_hover_background(&self) -> Color {
        self.palette().control_hover
    }

    fn control_padding(&self) -> Insets {
        let scale = self.appearance().spacing_scale;
        let vertical = 6.0 + scale * 1.5;
        let horizontal = 10.0 + scale * 3.0;
        Insets::new(vertical, horizontal, vertical, horizontal)
    }

    fn spacing(&self) -> f32 {
        BASE_SPACING * self.appearance().spacing_scale
    }

    fn font_size(&self) -> f32 {
        BASE_FONT_SIZE * self.appearance().font_scale
    }

    fn corner_radius(&self) -> f32 {
        self.appearance().radius
    }

    fn animation_duration(&self) -> Duration {
        if self.appearance().animations {
            Duration::from_millis(150)
        } else {
            Duration::ZERO
        }
    }
}

// ---------------------------------------------------------------------------
// Application shell
// ---------------------------------------------------------------------------

struct StyleShowcase {
    runtime: UiRuntime,
    size: Size,
    appearance: Rc<RefCell<Appearance>>,
    dirty: Rc<Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

impl StyleShowcase {
    fn new(selected_tab: usize) -> Self {
        let size = valid_size(WINDOW_WIDTH, WINDOW_HEIGHT);
        let appearance = Rc::new(RefCell::new(Appearance::default()));
        let dirty = Rc::new(Cell::new(false));
        let mut environment = UiEnvironment::new(ShowcaseTheme {
            appearance: Rc::clone(&appearance),
        });
        environment.set_animations_enabled(appearance.borrow().animations);
        let mut runtime = UiRuntime::with_environment(ShowcaseShell, environment);
        let root = runtime.root();
        let mut subscriptions = Vec::new();

        let menu = MenuBar::new(["File", "Edit", "View", "Help"]);
        subscriptions.push(menu.activated().subscribe(|index| {
            let names = ["File", "Edit", "View", "Help"];
            println!("MenuBar: opened section \"{}\".", names[*index]);
        }));
        runtime
            .append_child(root, menu)
            .expect("showcase root exists");

        let toolbar = runtime
            .append_child(
                root,
                Toolbar::new().with_padding(Insets::new(6.0, 10.0, 6.0, 10.0)),
            )
            .expect("showcase root exists");
        for (label, message) in [
            ("+ New", "Created a new document."),
            ("Save", "State saved."),
            ("Undo", "Reverted the last action."),
            ("Settings", "Open the \"Appearance\" tab."),
        ] {
            append_tool_button(&mut runtime, &mut subscriptions, toolbar, label, message);
        }

        let mut tabs = TabBar::new([
            "Overview",
            "Components",
            "Appearance",
            "Layout",
            "Workspace",
        ]);
        tabs.set_selected(Some(selected_tab.min(4)));
        subscriptions.push(
            tabs.changed()
                .subscribe(|index| println!("TabBar: opened page {}.", index + 1)),
        );
        let tabs = runtime
            .append_child(root, tabs)
            .expect("showcase root exists");

        append_overview_page(&mut runtime, &appearance, tabs);
        append_components_page(&mut runtime, &mut subscriptions, tabs);
        append_appearance_page(&mut runtime, &mut subscriptions, &appearance, &dirty, tabs);
        append_layout_page(&mut runtime, tabs);
        append_workspace_page(&mut runtime, tabs);

        runtime
            .append_child(root, StatusBar::new(Rc::clone(&appearance)))
            .expect("showcase root exists");

        let mut app = Self {
            runtime,
            size,
            appearance,
            dirty,
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

impl WindowApplication for StyleShowcase {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Drawen — Style Lab / Appearance Studio", self.size)
    }

    fn window_event(&mut self, event: WindowEvent) -> WindowAction {
        match event {
            WindowEvent::CloseRequested => WindowAction::Exit,
            WindowEvent::Resized(size) => {
                self.layout(size);
                WindowAction::RequestRedraw
            }
            WindowEvent::Input(event) => {
                let _ = self.runtime.dispatch_event(&event);
                if self.dirty.replace(false) {
                    let animations = self.appearance.borrow().animations;
                    self.runtime
                        .environment_mut()
                        .set_animations_enabled(animations);
                    self.runtime.invalidate_layout();
                }
                if self.runtime.take_redraw_request() {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
            }
            // Lifecycle changes only affect whether a frame is worth drawing;
            // the platform asks for a redraw when content becomes visible.
            WindowEvent::Visible(_) | WindowEvent::Minimized(_) | WindowEvent::Occluded(_) => {
                WindowAction::None
            }
            WindowEvent::Maximized(_) | WindowEvent::Fullscreen(_) => WindowAction::RequestRedraw,
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

    fn next_frame_in(&self) -> Option<Duration> {
        self.runtime.next_frame_in()
    }
}

/// Menu bar, quick actions, tabbed content area, and status bar.
struct ShowcaseShell;

impl Widget for ShowcaseShell {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let (menu, menu_layout) = context
            .layout_child(
                0,
                Constraints::loose(valid_size(size.width(), MENU_MAX_HEIGHT)),
            )
            .expect("shell menu exists");
        let menu_height = menu_layout.size().height().min(size.height());

        let remaining = (size.height() - menu_height).max(0.0);
        let (toolbar, toolbar_layout) = context
            .layout_child(
                1,
                Constraints::loose(valid_size(size.width(), TOOLBAR_MAX_HEIGHT.min(remaining))),
            )
            .expect("shell toolbar exists");
        let toolbar_height = toolbar_layout.size().height().min(remaining);

        let status_height =
            STATUS_HEIGHT.min((size.height() - menu_height - toolbar_height).max(0.0));
        let tabs_height = (size.height() - menu_height - toolbar_height - status_height).max(0.0);
        let (tabs, _) = context
            .layout_child(2, tight(size.width(), tabs_height))
            .expect("shell tabs exists");
        let (status, _) = context
            .layout_child(3, tight(size.width(), status_height))
            .expect("shell status exists");

        LayoutResult::with_children(
            size,
            vec![
                ChildLayout::new(menu, Point::ZERO),
                ChildLayout::new(toolbar, Point::new(0.0, menu_height)),
                ChildLayout::new(tabs, Point::new(0.0, menu_height + toolbar_height)),
                ChildLayout::new(
                    status,
                    Point::new(0.0, menu_height + toolbar_height + tabs_height),
                ),
            ],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, environment.theme().background());
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// A tab page that paints the theme background behind a scrollable child.
struct Page;

impl Widget for Page {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let mut children = Vec::new();
        if context.child_count() == 1 {
            let (id, _) = context
                .layout_child(0, tight(size.width(), size.height()))
                .expect("page scroll child exists");
            children.push(ChildLayout::new(id, Point::ZERO));
        }
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, environment.theme().background());
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn clips_children(&self) -> bool {
        true
    }
}

/// Vertical stack with a real gap and optional padding.
struct VStack {
    gap: f32,
    padding: Insets,
}

impl VStack {
    const fn new(gap: f32) -> Self {
        Self {
            gap,
            padding: Insets::ZERO,
        }
    }

    const fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }
}

impl Widget for VStack {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            constraints.min().width()
        };
        let inner_width = (width - self.padding.horizontal()).max(0.0);
        let mut cursor = self.padding.top;
        let mut widest: f32 = 0.0;
        let mut children = Vec::with_capacity(context.child_count());
        for index in 0..context.child_count() {
            let (id, layout) = context
                .layout_child(
                    index,
                    Constraints::loose(valid_size(inner_width, f32::INFINITY)),
                )
                .expect("stack child index is valid");
            let child = layout.size();
            children.push(ChildLayout::new(id, Point::new(self.padding.left, cursor)));
            cursor += child.height();
            if index + 1 < context.child_count() {
                cursor += self.gap;
            }
            widest = widest.max(child.width());
        }
        let content_width = if max.width().is_finite() {
            width
        } else {
            widest + self.padding.horizontal()
        };
        let height = (cursor + self.padding.bottom).max(0.0);
        LayoutResult::with_children(
            constraints.constrain(valid_size(content_width, height)),
            children,
        )
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// Horizontal stack. In `equal` mode children receive equal-width cells.
struct HStack {
    gap: f32,
    equal: bool,
}

impl HStack {
    const fn new(gap: f32) -> Self {
        Self { gap, equal: false }
    }

    const fn with_equal(mut self, equal: bool) -> Self {
        self.equal = equal;
        self
    }
}

impl Widget for HStack {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let count = context.child_count();
        let total_gap = if count > 1 {
            self.gap * usize_to_f32(count - 1)
        } else {
            0.0
        };
        let width = if max.width().is_finite() {
            max.width()
        } else {
            constraints.min().width()
        };
        let cell_width = if self.equal && count > 0 {
            ((width - total_gap) / usize_to_f32(count)).max(0.0)
        } else {
            width
        };
        let mut children = Vec::with_capacity(count);
        let mut cursor = 0.0;
        let mut height: f32 = 0.0;
        for index in 0..count {
            let child_width = if self.equal { cell_width } else { width };
            let (id, layout) = context
                .layout_child(
                    index,
                    Constraints::loose(valid_size(child_width, f32::INFINITY)),
                )
                .expect("row child index is valid");
            let origin_x = if self.equal {
                usize_to_f32(index) * (cell_width + self.gap)
            } else {
                cursor
            };
            children.push(ChildLayout::new(id, Point::new(origin_x, 0.0)));
            height = height.max(layout.size().height());
            cursor = origin_x + layout.size().width() + self.gap;
        }
        let content_width = if self.equal || max.width().is_finite() {
            width
        } else {
            (cursor - self.gap).max(0.0)
        };
        LayoutResult::with_children(
            constraints.constrain(valid_size(content_width, height)),
            children,
        )
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// A theme-aware card surface with one content child.
struct SurfaceCard {
    elevation: f32,
}

impl SurfaceCard {
    const fn new(elevation: f32) -> Self {
        Self { elevation }
    }
}

impl Widget for SurfaceCard {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let theme = context.environment().theme();
        let padding = (theme.spacing() * 2.0).max(14.0);
        let max = constraints.max();
        let child_max = valid_size(
            (max.width() - padding * 2.0).max(0.0),
            (max.height() - padding * 2.0).max(0.0),
        );
        let mut children = Vec::new();
        let child_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("card content exists");
            children.push(ChildLayout::new(id, Point::new(padding, padding)));
            layout.size()
        } else {
            Size::ZERO
        };
        LayoutResult::with_children(
            constraints.constrain(valid_size(
                child_size.width() + padding * 2.0,
                child_size.height() + padding * 2.0,
            )),
            children,
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let radius = theme.corner_radius() * 1.35;
        context.draw_shadow(bounds, radius, self.elevation, theme.shadow());
        context.fill_rounded_rect(bounds, radius, theme.raised_surface());
        context.stroke_rounded_rect(bounds, radius, 1.0, theme.border());
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// Gives its single child a fixed height while keeping the available width.
struct FixedHeight {
    height: f32,
}

impl FixedHeight {
    const fn new(height: f32) -> Self {
        Self { height }
    }
}

impl Widget for FixedHeight {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            constraints.min().width().max(320.0)
        };
        let height = self.height.min(max.height().max(0.0));
        let mut children = Vec::new();
        if context.child_count() == 1 {
            let (id, _) = context
                .layout_child(0, tight(width, height))
                .expect("fixed-height child exists");
            children.push(ChildLayout::new(id, Point::ZERO));
        }
        LayoutResult::with_children(valid_size(width, height), children)
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Theme-aware text and presentation widgets
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextTone {
    Primary,
    Muted,
    OnAccent,
}

impl TextTone {
    fn color(self, environment: &UiEnvironment) -> Color {
        let theme = environment.theme();
        match self {
            Self::Primary => theme.foreground(),
            Self::Muted => theme.muted_foreground(),
            Self::OnAccent => theme.accent_foreground(),
        }
    }
}

/// Text that reshapes against the live theme font and color during layout.
struct Label {
    text: String,
    scale: f32,
    tone: TextTone,
    layout: Option<TextLayout>,
}

impl Label {
    fn new(text: impl Into<String>, scale: f32, tone: TextTone) -> Self {
        Self {
            text: text.into(),
            scale,
            tone,
            layout: None,
        }
    }
}

impl Widget for Label {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let environment = context.environment();
        let max = constraints.max();
        let wrap = max.width().is_finite().then_some(max.width());
        let layout = layout_text(
            environment,
            &self.text,
            self.scale,
            self.tone.color(environment),
            wrap,
        );
        let size = layout.size();
        self.layout = Some(layout);
        LayoutResult::new(constraints.constrain(size))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        if let Some(layout) = &self.layout {
            context.draw_text(layout.clone(), bounds.origin);
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// A row of rounded pill chips with theme-tinted accent fills.
struct Chips {
    labels: Vec<&'static str>,
    layouts: Vec<TextLayout>,
    chip_widths: Vec<f32>,
    height: f32,
}

impl Chips {
    fn new(labels: &[&'static str]) -> Self {
        Self {
            labels: labels.to_vec(),
            layouts: Vec::new(),
            chip_widths: Vec::new(),
            height: 0.0,
        }
    }
}

impl Widget for Chips {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let environment = context.environment();
        let theme = environment.theme();
        let padding = theme.spacing() * 1.4;
        let gap = theme.spacing() * 0.75;
        self.layouts.clear();
        self.chip_widths.clear();
        let mut total = 0.0;
        for label in &self.labels {
            let layout = layout_text(environment, label, 0.84, theme.accent(), None);
            let width = layout.size().width() + padding;
            total += width + gap;
            self.chip_widths.push(width);
            self.layouts.push(layout);
        }
        if !self.labels.is_empty() {
            total -= gap;
        }
        self.height = (theme.font_size() * 1.9).max(24.0);
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            total
        };
        LayoutResult::new(constraints.constrain(valid_size(width, self.height)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let gap = theme.spacing() * 0.75;
        let radius = self.height / 2.0;
        let mut cursor = 0.0;
        for ((layout, width), _) in self.layouts.iter().zip(&self.chip_widths).zip(&self.labels) {
            let chip = Rect::new(
                Point::new(bounds.origin.x + cursor, bounds.origin.y),
                valid_size(*width, self.height),
            );
            context.fill_rounded_rect(chip, radius, theme.accent().with_alpha(0.14));
            context.stroke_rounded_rect(chip, radius, 1.0, theme.accent().with_alpha(0.34));
            let text_height = layout.size().height();
            context.draw_text(
                layout.clone(),
                Point::new(
                    chip.origin.x + (chip.size.width() - layout.size().width()) / 2.0,
                    chip.origin.y + (self.height - text_height) / 2.0,
                ),
            );
            cursor += width + gap;
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// A compact dashboard metric.
struct StatCard {
    value: &'static str,
    caption: &'static str,
    value_layout: Option<TextLayout>,
    caption_layout: Option<TextLayout>,
}

impl StatCard {
    const fn new(value: &'static str, caption: &'static str) -> Self {
        Self {
            value,
            caption,
            value_layout: None,
            caption_layout: None,
        }
    }
}

impl Widget for StatCard {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let environment = context.environment();
        let theme = environment.theme();
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            180.0
        };
        let height = (theme.font_size() * 4.4).max(86.0);
        self.value_layout = Some(layout_text(
            environment,
            self.value,
            1.9,
            theme.accent(),
            Some((width - 28.0).max(0.0)),
        ));
        self.caption_layout = Some(layout_text(
            environment,
            self.caption,
            0.84,
            theme.muted_foreground(),
            Some((width - 28.0).max(0.0)),
        ));
        LayoutResult::new(constraints.constrain(valid_size(width, height)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let radius = theme.corner_radius() * 1.2;
        context.fill_rounded_rect(bounds, radius, theme.surface());
        context.stroke_rounded_rect(bounds, radius, 1.0, theme.border());
        let padding = 14.0;
        if let Some(layout) = &self.value_layout {
            context.draw_text(
                layout.clone(),
                Point::new(bounds.origin.x + padding, bounds.origin.y + padding),
            );
        }
        if let Some(layout) = &self.caption_layout {
            context.draw_text(
                layout.clone(),
                Point::new(
                    bounds.origin.x + padding,
                    bounds.origin.y + bounds.size.height() - padding - layout.size().height(),
                ),
            );
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// Live palette preview painted from the current semantic theme.
struct PalettePreview;

impl Widget for PalettePreview {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            360.0
        };
        let height = constraints.constrain(valid_size(width, 96.0)).height();
        LayoutResult::new(valid_size(width, height))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let radius = theme.corner_radius();
        context.fill_rounded_rect(bounds, radius, theme.surface());
        context.stroke_rounded_rect(bounds, radius, 1.0, theme.border());

        let entries = [
            ("background", theme.background()),
            ("surface", theme.surface()),
            ("raised", theme.raised_surface()),
            ("control", theme.control_background()),
            ("accent", theme.accent()),
            ("text", theme.foreground()),
        ];
        let gap = 8.0;
        let inset = 12.0;
        let available = (bounds.size.width() - inset * 2.0 - gap * 5.0).max(0.0);
        let cell = (available / 6.0).max(0.0);
        for (index, (label, color)) in entries.iter().enumerate() {
            let x = bounds.origin.x + inset + usize_to_f32(index) * (cell + gap);
            let swatch = Rect::new(
                Point::new(x, bounds.origin.y + inset),
                valid_size(cell, 42.0),
            );
            context.fill_rounded_rect(swatch, (radius * 0.7).max(4.0), *color);
            context.stroke_rounded_rect(swatch, (radius * 0.7).max(4.0), 1.0, theme.border());
            let text = layout_text(
                environment,
                label,
                0.72,
                theme.muted_foreground(),
                Some(cell),
            );
            context.draw_text(
                text.clone(),
                Point::new(
                    x + (cell - text.size().width()) / 2.0,
                    bounds.origin.y + inset + 48.0,
                ),
            );
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Settings row
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Metric {
    FontSize,
    Spacing,
    Radius,
    Accent,
    Preset,
    Animation,
}

impl Metric {
    fn text(self, appearance: &Appearance) -> String {
        match self {
            Self::FontSize => format!("{:.1} px", BASE_FONT_SIZE * appearance.font_scale),
            Self::Spacing => format!("{:.1} px", BASE_SPACING * appearance.spacing_scale),
            Self::Radius => format!("{:.0} px", appearance.radius),
            Self::Accent => format!(
                "{} · {:.0}°",
                accent_name(appearance.accent_hue),
                appearance.accent_hue
            ),
            Self::Preset => appearance.preset.label().to_owned(),
            Self::Animation => {
                if appearance.animations {
                    "Smooth".to_owned()
                } else {
                    "Instant".to_owned()
                }
            }
        }
    }

    fn swatch(self, appearance: &Appearance) -> Option<Color> {
        match self {
            Self::Accent => Some(accent_color(
                appearance.preset.is_dark(),
                appearance.accent_hue,
            )),
            _ => None,
        }
    }
}

/// Title / description / live value header above one control.
struct FormRow {
    title: String,
    description: Option<String>,
    metric: Option<Metric>,
    static_metric: Option<String>,
    appearance: Option<Rc<RefCell<Appearance>>>,
    align_end: bool,
    stretch: bool,
    title_layout: Option<TextLayout>,
    description_layout: Option<TextLayout>,
    metric_layout: Option<TextLayout>,
    metric_swatch: Option<Color>,
    title_height: f32,
    description_y: f32,
    metric_x: f32,
}

impl FormRow {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            metric: None,
            static_metric: None,
            appearance: None,
            align_end: false,
            stretch: true,
            title_layout: None,
            description_layout: None,
            metric_layout: None,
            metric_swatch: None,
            title_height: 0.0,
            description_y: 0.0,
            metric_x: 0.0,
        }
    }

    fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    fn with_metric(mut self, metric: Metric, appearance: Rc<RefCell<Appearance>>) -> Self {
        self.metric = Some(metric);
        self.appearance = Some(appearance);
        self
    }

    fn with_static_metric(mut self, metric: impl Into<String>) -> Self {
        self.static_metric = Some(metric.into());
        self
    }

    const fn align_end(mut self) -> Self {
        self.align_end = true;
        self
    }

    const fn without_stretch(mut self) -> Self {
        self.stretch = false;
        self
    }
}

impl Widget for FormRow {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let environment = context.environment();
        let theme = environment.theme();
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            constraints.min().width().max(320.0)
        };

        let appearance = self.appearance.as_ref().map(|value| *value.borrow());
        let metric_text = self
            .metric
            .zip(appearance)
            .map(|(metric, appearance)| metric.text(&appearance))
            .or_else(|| self.static_metric.clone());

        let mut metric_width = 0.0;
        self.metric_layout = None;
        self.metric_swatch = None;
        if let Some(text) = metric_text {
            let layout = layout_text(environment, &text, 0.86, theme.accent(), None);
            metric_width = layout.size().width();
            self.metric_swatch = self
                .metric
                .zip(appearance)
                .and_then(|(metric, appearance)| metric.swatch(&appearance));
            self.metric_layout = Some(layout);
        }
        let metric_space = if self.metric_layout.is_some() {
            metric_width + 26.0
        } else {
            0.0
        };
        self.metric_x = (width - metric_width).max(0.0);

        let title_wrap = (width - metric_space).max(0.0);
        let title = layout_text(
            environment,
            &self.title,
            1.0,
            theme.foreground(),
            Some(title_wrap),
        );
        self.title_height = title.size().height();
        self.title_layout = Some(title);

        self.description_y = self.title_height;
        self.description_layout = None;
        if let Some(description) = &self.description {
            let layout = layout_text(
                environment,
                description,
                0.84,
                theme.muted_foreground(),
                Some(width),
            );
            self.description_y += 3.0;
            self.title_height += 3.0 + layout.size().height();
            self.description_layout = Some(layout);
        }

        let gap = theme.spacing() * 0.9;
        let child_y = self.title_height + gap;
        let child_max_height = if max.height().is_finite() {
            (max.height() - child_y).max(0.0)
        } else {
            f32::INFINITY
        };
        let child_constraints = if self.stretch {
            Constraints::new(valid_size(width, 0.0), valid_size(width, child_max_height))
                .expect("stretched form constraints are valid")
        } else {
            Constraints::loose(valid_size(width, child_max_height))
        };
        let mut children = Vec::new();
        let mut total = self.title_height;
        if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, child_constraints)
                .expect("form control exists");
            let child = layout.size();
            let x = if self.align_end && width.is_finite() {
                (width - child.width()).max(0.0)
            } else {
                0.0
            };
            children.push(ChildLayout::new(id, Point::new(x, child_y)));
            total = child_y + child.height();
        }
        LayoutResult::with_children(constraints.constrain(valid_size(width, total)), children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        if let Some(layout) = &self.title_layout {
            context.draw_text(layout.clone(), bounds.origin);
        }
        if let Some(layout) = &self.metric_layout {
            let mut x = bounds.origin.x + self.metric_x;
            if let Some(color) = self.metric_swatch {
                let swatch = Rect::new(
                    Point::new(x - 18.0, bounds.origin.y + (self.title_height - 11.0) / 2.0),
                    valid_size(11.0, 11.0),
                );
                context.fill_rounded_rect(swatch, 3.0, color);
                x += 0.0;
            }
            context.draw_text(layout.clone(), Point::new(x, bounds.origin.y));
        }
        if let Some(layout) = &self.description_layout {
            context.draw_text(
                layout.clone(),
                Point::new(bounds.origin.x, bounds.origin.y + self.description_y),
            );
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Page content: overview
// ---------------------------------------------------------------------------

fn append_overview_page(
    runtime: &mut UiRuntime,
    appearance: &Rc<RefCell<Appearance>>,
    tabs: drawen::WidgetId,
) {
    let stack = append_scroll_page(runtime, tabs);

    let hero = runtime
        .append_child(stack, SurfaceCard::new(12.0))
        .expect("overview stack exists");
    let hero_column = runtime
        .append_child(hero, VStack::new(12.0))
        .expect("hero card exists");
    append_label(
        runtime,
        hero_column,
        "Drawen UI · Style Lab",
        2.2,
        TextTone::Primary,
    );
    append_label(
        runtime,
        hero_column,
        "An experimental interface laboratory: theme, typography, density, and geometry change live while the application runs — similar to Qt and AUI.",
        1.02,
        TextTone::Muted,
    );
    runtime
        .append_child(
            hero_column,
            Chips::new(&[
                "Live palette",
                "Qt/AUI approach",
                "EN / RU input",
                "Retained tree",
            ]),
        )
        .expect("hero column exists");
    append_divider(runtime, hero_column);

    let stats = runtime
        .append_child(hero_column, HStack::new(14.0).with_equal(true))
        .expect("hero column exists");
    for (value, caption) in [
        ("3", "palettes"),
        ("40+", "components"),
        ("5", "showcase pages"),
    ] {
        runtime
            .append_child(stats, StatCard::new(value, caption))
            .expect("stats row exists");
    }

    let palette_card = append_card(
        runtime,
        stack,
        "Palette",
        "Every color below is read from the current theme. Open the \"Appearance\" page to change them.",
    );
    runtime
        .append_child(palette_card, PalettePreview)
        .expect("palette card exists");

    let atlas = append_card(
        runtime,
        stack,
        "Raster and typography",
        "The image is drawn through the display list, and labels are reshaped when the theme changes.",
    );
    runtime
        .append_child(atlas, Image::with_size(drawen_logo(), 72.0, 72.0))
        .expect("atlas card exists");
    append_label(
        runtime,
        atlas,
        "Logo · 16×16 RGBA · scales without losing paint commands",
        0.86,
        TextTone::Muted,
    );
    let _ = appearance;
}

// ---------------------------------------------------------------------------
// Page content: components
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn append_components_page(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    tabs: drawen::WidgetId,
) {
    let stack = append_scroll_page(runtime, tabs);

    let text_card = append_card(
        runtime,
        stack,
        "Text",
        "Single-line and multiline editing. Type in English or Russian — the platform forwards the layout through KeyEvent::text.",
    );
    let field = TextField::with_text("Hello, Drawen! Edit this text.");
    subscriptions.push(
        field
            .changed()
            .subscribe(|text| println!("TextField: {text}")),
    );
    append_form(
        runtime,
        text_card,
        FormRow::new("Single-line field").with_description(
            "Click the field and start typing — the caret, selection, and IME work as expected.",
        ),
        field,
    );

    let editor = TextEdit::with_text(
        "This is a multiline TextEdit.\nCheck line wrapping, selection, and scrolling.\nEnglish text: The quick brown fox jumps over the lazy dog.\nUnicode text: Ελληνικά · 日本語 · العربية.",
    );
    subscriptions.push(
        editor
            .changed()
            .subscribe(|text| println!("TextEdit: {} characters", text.chars().count())),
    );
    append_form(
        runtime,
        text_card,
        FormRow::new("Multiline editor").with_description(
            "TextEdit reuses TextField's grapheme navigation, undo, and clipboard behavior.",
        ),
        editor,
    );

    let choice_card = append_card(
        runtime,
        stack,
        "Choice",
        "Checkboxes, switches, lists, segments, and sliders — every element reads the same theme.",
    );
    let checkbox = append_form(
        runtime,
        choice_card,
        FormRow::new("Checkbox")
            .with_description("A binary state with focus and keyboard activation."),
        Checkbox::with_checked(true),
    );
    append_label(
        runtime,
        checkbox,
        "Show hover hints",
        0.96,
        TextTone::Primary,
    );
    append_form(
        runtime,
        choice_card,
        FormRow::new("Switch")
            .with_description("Applies a setting immediately.")
            .align_end()
            .without_stretch(),
        Switch::with_checked(true),
    );
    let mut dropdown = Dropdown::new(["Auto", "Compact", "Roomy"]);
    dropdown.set_selected(Some(0));
    subscriptions.push(
        dropdown
            .changed()
            .subscribe(|index| println!("Dropdown: selected option {index}")),
    );
    append_form(
        runtime,
        choice_card,
        FormRow::new("Dropdown").with_description("The list opens inside the retained tree."),
        dropdown,
    );
    let segments = SegmentedControl::new(["Left", "Center", "Right"]).with_selected(1);
    subscriptions.push(
        segments
            .changed()
            .subscribe(|index| println!("SegmentedControl: segment {index}")),
    );
    append_form(
        runtime,
        choice_card,
        FormRow::new("Segmented control")
            .with_description("Mutually exclusive options on one line."),
        segments,
    );
    let slider = Slider::with_value(0.0, 100.0, 64.0);
    subscriptions.push(
        slider
            .changed()
            .subscribe(|value| println!("Slider: {value:.0}%")),
    );
    append_form(
        runtime,
        choice_card,
        FormRow::new("Slider")
            .with_description("A continuous range with keyboard control.")
            .with_static_metric("64%"),
        slider,
    );

    let radio_card = append_card(
        runtime,
        stack,
        "Radio buttons",
        "RadioGroup stores the selected value separately from the widgets and emits a signal.",
    );
    let group = RadioGroup::new();
    group.set_selected(&"All");
    subscriptions.push(
        group
            .changed()
            .subscribe(|value| println!("RadioGroup: {}", *value)),
    );
    let radio_row = runtime
        .append_child(
            radio_card,
            FormRow::new("Notifications")
                .with_description("Select one option — the others reset automatically."),
        )
        .expect("radio card exists");
    let radios = runtime
        .append_child(radio_row, VStack::new(7.0))
        .expect("radio row exists");
    for option in ["All", "Important", "None"] {
        let radio = runtime
            .append_child(radios, RadioButton::new(group.clone(), option))
            .expect("radio stack exists");
        append_label(runtime, radio, option, 0.96, TextTone::Primary);
    }

    let action_card = append_card(
        runtime,
        stack,
        "Actions and status",
        "Standard buttons, indicators, badges, avatars, and semantic alerts.",
    );
    let actions = runtime
        .append_child(action_card, HStack::new(10.0))
        .expect("action card exists");
    for (label, message) in [
        ("Primary", "Primary action"),
        ("Secondary", "Secondary action"),
        ("Danger", "Dangerous action"),
    ] {
        append_button(runtime, subscriptions, actions, label, message);
    }
    append_form(
        runtime,
        action_card,
        FormRow::new("Progress").with_static_metric("72%"),
        ProgressBar::with_progress(0.72),
    );
    let indicators = runtime
        .append_child(action_card, HStack::new(18.0))
        .expect("action card exists");
    runtime
        .append_child(indicators, Spinner::new().with_diameter(28.0))
        .expect("indicator row exists");
    runtime
        .append_child(indicators, Skeleton::new().with_size(170.0, 18.0))
        .expect("indicator row exists");
    runtime
        .append_child(
            indicators,
            Avatar::new()
                .with_diameter(42.0)
                .with_initials("TR")
                .with_status(AvatarStatus::Online)
                .with_label("Drawen"),
        )
        .expect("indicator row exists");

    let badges = runtime
        .append_child(action_card, HStack::new(8.0))
        .expect("action card exists");
    for (tone, text) in [
        (BadgeTone::Neutral, "Neutral"),
        (BadgeTone::Accent, "Accent"),
        (BadgeTone::Success, "Success"),
        (BadgeTone::Warning, "Warning"),
        (BadgeTone::Danger, "Error"),
    ] {
        append_badge(runtime, badges, tone, text);
    }

    let alert_card = append_card(
        runtime,
        stack,
        "Notifications",
        "Alert uses the theme's semantic colors: accent, success, warning, and danger.",
    );
    for (tone, title, message) in [
        (
            AlertTone::Info,
            "Information",
            "Theme tokens pick up the new accent automatically.",
        ),
        (
            AlertTone::Success,
            "Done",
            "Appearance changes apply without restarting the application.",
        ),
        (
            AlertTone::Warning,
            "Warning",
            "A very small radius can reduce the readability of borders.",
        ),
        (
            AlertTone::Danger,
            "Error",
            "This is how a destructive action looks in the current palette.",
        ),
    ] {
        runtime
            .append_child(
                alert_card,
                Alert::new()
                    .with_tone(tone)
                    .with_title(title)
                    .with_message(message),
            )
            .expect("alert card exists");
    }

    let expander_card = append_card(
        runtime,
        stack,
        "Expandable sections",
        "Expander keeps its body in the tree but excludes it from layout, paint, and hit testing while collapsed.",
    );
    let expander = runtime
        .append_child(
            expander_card,
            Expander::new("Advanced options").with_expanded(true),
        )
        .expect("expander card exists");
    let body = runtime
        .append_child(expander, VStack::new(8.0))
        .expect("expander exists");
    append_label(
        runtime,
        body,
        "A section can contain any child widgets: forms, lists, and tables.",
        0.92,
        TextTone::Muted,
    );
    append_label(
        runtime,
        body,
        "Space and Enter toggle the state, as in native dialogs.",
        0.92,
        TextTone::Muted,
    );
}

// ---------------------------------------------------------------------------
// Page content: appearance
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn append_appearance_page(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    appearance: &Rc<RefCell<Appearance>>,
    dirty: &Rc<Cell<bool>>,
    tabs: drawen::WidgetId,
) {
    let stack = append_scroll_page(runtime, tabs);

    let card = append_card(
        runtime,
        stack,
        "Appearance",
        "Tune the interface. The values below change the semantic theme used by every standard widget.",
    );

    let preset =
        SegmentedControl::new(Preset::LABELS).with_selected(appearance.borrow().preset.index());
    subscriptions.push(preset.changed().subscribe({
        let appearance = Rc::clone(appearance);
        let dirty = Rc::clone(dirty);
        move |index| {
            if let Some(next) = Preset::ALL.get(*index) {
                appearance.borrow_mut().preset = *next;
                dirty.set(true);
            }
        }
    }));
    append_form(
        runtime,
        card,
        FormRow::new("Palette")
            .with_description("Base surfaces, contrast, and shadows.")
            .with_metric(Metric::Preset, Rc::clone(appearance)),
        preset,
    );

    let accent = Slider::with_value(0.0, 360.0, appearance.borrow().accent_hue);
    subscriptions.push(accent.changed().subscribe({
        let appearance = Rc::clone(appearance);
        let dirty = Rc::clone(dirty);
        move |value| {
            let mut settings = appearance.borrow_mut();
            settings.accent_hue = *value;
            dirty.set(true);
        }
    }));
    append_form(
        runtime,
        card,
        FormRow::new("Accent color")
            .with_description("Hue in HSL space; saturation and lightness adapt to the palette.")
            .with_metric(Metric::Accent, Rc::clone(appearance)),
        accent,
    );

    let font = Slider::with_value(0.8, 1.6, appearance.borrow().font_scale);
    subscriptions.push(font.changed().subscribe({
        let appearance = Rc::clone(appearance);
        let dirty = Rc::clone(dirty);
        move |value| {
            let mut settings = appearance.borrow_mut();
            settings.font_scale = *value;
            dirty.set(true);
        }
    }));
    append_form(
        runtime,
        card,
        FormRow::new("Font size")
            .with_description(
                "The theme's base size. Every standard control recalculates text and height.",
            )
            .with_metric(Metric::FontSize, Rc::clone(appearance)),
        font,
    );

    let spacing = Slider::with_value(0.6, 1.8, appearance.borrow().spacing_scale);
    subscriptions.push(spacing.changed().subscribe({
        let appearance = Rc::clone(appearance);
        let dirty = Rc::clone(dirty);
        move |value| {
            let mut settings = appearance.borrow_mut();
            settings.spacing_scale = *value;
            dirty.set(true);
        }
    }));
    append_form(
        runtime,
        card,
        FormRow::new("Density")
            .with_description("Scales spacing, gaps, and control padding.")
            .with_metric(Metric::Spacing, Rc::clone(appearance)),
        spacing,
    );

    let radius = Slider::with_value(0.0, 22.0, appearance.borrow().radius);
    subscriptions.push(radius.changed().subscribe({
        let appearance = Rc::clone(appearance);
        let dirty = Rc::clone(dirty);
        move |value| {
            let mut settings = appearance.borrow_mut();
            settings.radius = *value;
            dirty.set(true);
        }
    }));
    append_form(
        runtime,
        card,
        FormRow::new("Corner radius")
            .with_description(
                "Radius of controls and cards. Zero produces a strict rectangular interface.",
            )
            .with_metric(Metric::Radius, Rc::clone(appearance)),
        radius,
    );

    let animations = Switch::with_checked(appearance.borrow().animations);
    subscriptions.push(animations.toggled().subscribe({
        let appearance = Rc::clone(appearance);
        let dirty = Rc::clone(dirty);
        move |enabled| {
            appearance.borrow_mut().animations = *enabled;
            dirty.set(true);
        }
    }));
    append_form(
        runtime,
        card,
        FormRow::new("Animations")
            .with_description(
                "Smooth hover, press, and focus transitions. Disable for reduced motion.",
            )
            .with_metric(Metric::Animation, Rc::clone(appearance))
            .align_end()
            .without_stretch(),
        animations,
    );

    append_divider(runtime, card);

    append_label(runtime, card, "Preview", 1.12, TextTone::Primary);
    runtime
        .append_child(card, PalettePreview)
        .expect("appearance card exists");

    let hint_card = append_card(
        runtime,
        stack,
        "How it works",
        "Drawen separates the semantic theme from DSS styles. The showcase changes only the theme, so every standard widget picks up the change through environment.theme().",
    );
    append_label(
        runtime,
        hint_card,
        "1. A control changes Appearance through a callback.\n2. The showcase sets a dirty flag and calls invalidate_layout().\n3. The next frame recalculates layout and resolves theme tokens again.",
        0.92,
        TextTone::Muted,
    );
}

// ---------------------------------------------------------------------------
// Page content: layout
// ---------------------------------------------------------------------------

fn append_layout_page(runtime: &mut UiRuntime, tabs: drawen::WidgetId) {
    let stack = append_scroll_page(runtime, tabs);

    let card = append_card(
        runtime,
        stack,
        "Splitter",
        "Drag the vertical splitter. Both panels receive new tight constraints instead of simply shifting.",
    );
    let frame = runtime
        .append_child(card, FixedHeight::new(310.0))
        .expect("layout card exists");
    let splitter = runtime
        .append_child(frame, Splitter::new(SplitAxis::Horizontal, 0.46))
        .expect("fixed splitter frame exists");
    runtime
        .append_child(
            splitter,
            PanelCard::new(
                "First panel",
                "Panels paint their background from theme.surface() with a slight accent tint, so switching palettes is immediately visible.",
                0.07,
            ),
        )
        .expect("splitter exists");
    runtime
        .append_child(
            splitter,
            PanelCard::new(
                "Second panel",
                "Splitter works with the pointer and keyboard while keeping focus inside the retained tree.",
                0.13,
            ),
        )
        .expect("splitter exists");

    let list_card = append_card(
        runtime,
        stack,
        "ListView",
        "The list is virtualized: only visible rows plus overscan are measured and painted.",
    );
    let list_frame = runtime
        .append_child(list_card, FixedHeight::new(286.0))
        .expect("list card exists");
    let list = runtime
        .append_child(list_frame, ListView::new(46.0))
        .expect("list frame exists");
    for index in 0..36 {
        runtime
            .append_child(list, ListRow { index })
            .expect("list view exists");
    }
}

/// A theme-tinted information panel used by the splitter and dock areas.
struct PanelCard {
    title: &'static str,
    body: &'static str,
    tint: f32,
    title_layout: Option<TextLayout>,
    body_layout: Option<TextLayout>,
}

impl PanelCard {
    const fn new(title: &'static str, body: &'static str, tint: f32) -> Self {
        Self {
            title,
            body,
            tint,
            title_layout: None,
            body_layout: None,
        }
    }
}

impl Widget for PanelCard {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let environment = context.environment();
        let theme = environment.theme();
        let size = available_size(constraints);
        let wrap = (size.width() - 36.0).max(0.0);
        self.title_layout = Some(layout_text(
            environment,
            self.title,
            1.28,
            theme.foreground(),
            Some(wrap),
        ));
        self.body_layout = Some(layout_text(
            environment,
            self.body,
            0.94,
            theme.muted_foreground(),
            Some(wrap),
        ));
        LayoutResult::new(size)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let radius = theme.corner_radius() * 1.2;
        let background = theme.surface().mix(theme.accent(), self.tint);
        context.fill_rounded_rect(bounds, radius, background);
        context.stroke_rounded_rect(bounds, radius, 1.0, theme.border());
        context.fill_rounded_rect(
            Rect::new(
                Point::new(bounds.origin.x + 18.0, bounds.origin.y + 18.0),
                valid_size(52.0, 4.0),
            ),
            2.0,
            theme.accent(),
        );
        if let Some(layout) = &self.title_layout {
            context.draw_text(
                layout.clone(),
                Point::new(bounds.origin.x + 18.0, bounds.origin.y + 32.0),
            );
        }
        if let Some(layout) = &self.body_layout {
            let title_height = self
                .title_layout
                .as_ref()
                .map_or(0.0, |layout| layout.size().height());
            context.draw_text(
                layout.clone(),
                Point::new(
                    bounds.origin.x + 18.0,
                    bounds.origin.y + 34.0 + title_height,
                ),
            );
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

/// A row painted by `ListView`.
struct ListRow {
    index: usize,
}

impl Widget for ListRow {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(available_size(constraints))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let background = if self.index.is_multiple_of(2) {
            theme.surface()
        } else {
            theme.raised_surface()
        };
        context.fill_rect(bounds, background);
        context.fill_rect(
            Rect::new(
                Point::new(
                    bounds.origin.x,
                    bounds.origin.y + bounds.size.height() - 1.0,
                ),
                valid_size(bounds.size.width(), 1.0),
            ),
            theme.border(),
        );
        let text = format!(
            "{:02}    virtualized row · retained tree · display list",
            self.index + 1
        );
        let layout = layout_text(
            environment,
            &text,
            0.96,
            theme.foreground(),
            Some((bounds.size.width() - 36.0).max(0.0)),
        );
        context.draw_text(
            layout.clone(),
            Point::new(
                bounds.origin.x + 18.0,
                bounds.origin.y + (bounds.size.height() - layout.size().height()) / 2.0,
            ),
        );
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Page content: dock workspace
// ---------------------------------------------------------------------------

fn append_workspace_page(runtime: &mut UiRuntime, tabs: drawen::WidgetId) {
    let page = runtime
        .append_child(tabs, WorkspacePage)
        .expect("tab bar exists");
    let layout = Rc::new(RefCell::new(
        WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Horizontal,
            0.25,
            LayoutNode::Panel {
                id: PanelId::from("navigator"),
            },
            LayoutNode::tabs(vec![PanelId::from("scene"), PanelId::from("inspector")]),
        ))
        .expect("showcase workspace layout is valid"),
    ));
    let mut dock = DockArea::new(layout);
    dock.register_panel(PanelId::from("navigator"))
        .expect("panel is unique");
    dock.register_panel(PanelId::from("scene"))
        .expect("panel is unique");
    dock.register_panel(PanelId::from("inspector"))
        .expect("panel is unique");
    let dock = runtime
        .append_child(page, dock)
        .expect("workspace page exists");
    for panel in [
        PanelCard::new(
            "Navigator",
            "DockArea projects the stable WorkspaceLayout model onto retained children.",
            0.05,
        ),
        PanelCard::new(
            "Scene",
            "Switch tabs and drag the splitter between groups.",
            0.10,
        ),
        PanelCard::new(
            "Inspector",
            "DockArea keeps the UI separate from the serializable panel model.",
            0.15,
        ),
    ] {
        runtime.append_child(dock, panel).expect("dock area exists");
    }
}

struct WorkspacePage;

impl Widget for WorkspacePage {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let inset = PAGE_PADDING
            .min(size.width() / 12.0)
            .min(size.height() / 12.0);
        let mut children = Vec::new();
        if context.child_count() == 1 {
            let (id, _) = context
                .layout_child(
                    0,
                    tight(
                        (size.width() - inset * 2.0).max(0.0),
                        (size.height() - inset * 2.0).max(0.0),
                    ),
                )
                .expect("workspace dock exists");
            children.push(ChildLayout::new(id, Point::new(inset, inset)));
        }
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, environment.theme().background());
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn clips_children(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

struct StatusBar {
    appearance: Rc<RefCell<Appearance>>,
}

impl StatusBar {
    const fn new(appearance: Rc<RefCell<Appearance>>) -> Self {
        Self { appearance }
    }
}

impl Widget for StatusBar {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        LayoutResult::new(constraints.constrain(valid_size(
            if max.width().is_finite() {
                max.width()
            } else {
                640.0
            },
            STATUS_HEIGHT,
        )))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        let appearance = *self.appearance.borrow();
        context.fill_rect(bounds, theme.surface());
        context.fill_rect(
            Rect::new(bounds.origin, valid_size(bounds.size.width(), 1.0)),
            theme.border(),
        );
        let left_text = format!(
            "Drawen Style Lab · {} · accent {} · {:.1} px · density {:.2}×",
            appearance.preset.label(),
            accent_name(appearance.accent_hue),
            BASE_FONT_SIZE * appearance.font_scale,
            appearance.spacing_scale
        );
        let right_text = "Tab: focus · Enter: activate";
        let left = layout_text(environment, &left_text, 0.8, theme.muted_foreground(), None);
        let right = layout_text(environment, right_text, 0.8, theme.muted_foreground(), None);
        context.draw_text(
            left.clone(),
            Point::new(
                bounds.origin.x + 14.0,
                bounds.origin.y + (bounds.size.height() - left.size().height()) / 2.0,
            ),
        );
        context.draw_text(
            right.clone(),
            Point::new(
                bounds.origin.x + bounds.size.width() - right.size().width() - 14.0,
                bounds.origin.y + (bounds.size.height() - right.size().height()) / 2.0,
            ),
        );
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn append_scroll_page(runtime: &mut UiRuntime, tabs: drawen::WidgetId) -> drawen::WidgetId {
    let page = runtime.append_child(tabs, Page).expect("tab bar exists");
    let scroll = runtime
        .append_child(page, ScrollView::vertical())
        .expect("page exists");
    runtime
        .append_child(
            scroll,
            VStack::new(18.0).with_padding(Insets::all(PAGE_PADDING)),
        )
        .expect("scroll view exists")
}

fn append_card(
    runtime: &mut UiRuntime,
    parent: drawen::WidgetId,
    title: &str,
    description: &str,
) -> drawen::WidgetId {
    let surface = runtime
        .append_child(parent, SurfaceCard::new(8.0))
        .expect("page stack exists");
    let column = runtime
        .append_child(surface, VStack::new(12.0))
        .expect("card surface exists");
    append_label(runtime, column, title, 1.42, TextTone::Primary);
    append_label(runtime, column, description, 0.92, TextTone::Muted);
    append_divider(runtime, column);
    column
}

fn append_label(
    runtime: &mut UiRuntime,
    parent: drawen::WidgetId,
    text: &str,
    scale: f32,
    tone: TextTone,
) -> drawen::WidgetId {
    runtime
        .append_child(parent, Label::new(text, scale, tone))
        .expect("label parent exists")
}

fn append_divider(runtime: &mut UiRuntime, parent: drawen::WidgetId) {
    runtime
        .append_child(parent, Divider::new())
        .expect("divider parent exists");
}

fn append_form<W>(
    runtime: &mut UiRuntime,
    parent: drawen::WidgetId,
    row: FormRow,
    control: W,
) -> drawen::WidgetId
where
    W: Widget + 'static,
{
    let row = runtime
        .append_child(parent, row)
        .expect("form parent exists");
    runtime.append_child(row, control).expect("form row exists")
}

fn append_button(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    parent: drawen::WidgetId,
    text: &str,
    message: &'static str,
) {
    let button = Button::new();
    subscriptions.push(
        button
            .activated()
            .subscribe(move |()| println!("{message}")),
    );
    let button = runtime
        .append_child(parent, button)
        .expect("button parent exists");
    append_label(runtime, button, text, 0.98, TextTone::Primary);
}

fn append_tool_button(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    toolbar: drawen::WidgetId,
    text: &'static str,
    message: &'static str,
) {
    let button = Button::new();
    subscriptions.push(
        button
            .activated()
            .subscribe(move |()| println!("{message}")),
    );
    let button = runtime
        .append_child(toolbar, button)
        .expect("toolbar exists");
    append_label(runtime, button, text, 0.94, TextTone::Primary);
}

fn append_badge(runtime: &mut UiRuntime, parent: drawen::WidgetId, tone: BadgeTone, text: &str) {
    let badge = runtime
        .append_child(parent, Badge::new().with_tone(tone))
        .expect("badge parent exists");
    let label_tone = match tone {
        BadgeTone::Accent | BadgeTone::Success | BadgeTone::Warning | BadgeTone::Danger => {
            TextTone::OnAccent
        }
        BadgeTone::Neutral => TextTone::Primary,
    };
    append_label(runtime, badge, text, 0.78, label_tone);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn layout_text(
    environment: &UiEnvironment,
    text: &str,
    scale: f32,
    color: Color,
    wrap_width: Option<f32>,
) -> TextLayout {
    let mut font = environment.default_font();
    font.set_pixel_size(environment.theme().font_size() * scale);
    environment
        .text_shaper()
        .layout(text, &TextStyle::new_with_font(font, color), wrap_width)
}

fn drawen_logo() -> RasterImage {
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let color = if x == 7 || y == 7 || (x > 3 && x < 12 && y > 3 && y < 12) {
                Color::rgba8(94, 234, 212, 255)
            } else if (x + y) % 3 == 0 {
                Color::rgba8(167, 139, 250, 255)
            } else {
                Color::rgba8(26, 36, 62, 255)
            };
            pixels.extend_from_slice(&[
                color_component(color.red),
                color_component(color.green),
                color_component(color.blue),
                color_component(color.alpha),
            ]);
        }
    }
    RasterImage::new(16, 16, pixels).expect("built-in logo has valid RGBA pixels")
}

fn color_component(value: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

fn available_size(constraints: Constraints) -> Size {
    let max = constraints.max();
    let min = constraints.min();
    let width = if max.width().is_finite() {
        max.width()
    } else {
        min.width()
    };
    let height = if max.height().is_finite() {
        max.height()
    } else {
        min.height()
    };
    constraints.constrain(valid_size(width, height))
}

fn tight(width: f32, height: f32) -> Constraints {
    Constraints::tight(valid_size(width, height)).expect("showcase child bounds are valid")
}

fn valid_size(width: f32, height: f32) -> Size {
    Size::new(width.max(0.0), height.max(0.0)).expect("showcase dimensions are valid")
}

fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}
