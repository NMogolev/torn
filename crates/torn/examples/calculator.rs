//! A dockable calculator studio that exercises Torn's retained UI runtime.
//!
//! Run with `cargo run -p torn --example calculator`.

use std::{cell::RefCell, rc::Rc, time::Instant};

use torn::{
    Color, Constraints, DockArea, DockAxis, EventContext, EventPhase, EventStatus, InputEvent, Key,
    LayoutContext, LayoutNode, LayoutResult, NamedKey, PanelId, Point, Rect, Size, Theme,
    UiEnvironment, UiRuntime, Widget, WorkspaceLayout,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
};

const WINDOW_WIDTH: f32 = 1_140.0;
const WINDOW_HEIGHT: f32 = 760.0;
const TOP_BAR_HEIGHT: f32 = 58.0;

#[derive(Clone, Copy)]
struct Palette {
    background: Color,
    card: Color,
    border: Color,
    display: Color,
    button: Color,
    subtle: Color,
    operator: Color,
    accent: Color,
    text: Color,
    muted: Color,
}

#[derive(Clone, Copy)]
enum ThemeChoice {
    Dark,
    Light,
    Gray,
    Orange,
}

impl ThemeChoice {
    const ALL: [Self; 4] = [Self::Dark, Self::Light, Self::Gray, Self::Orange];

    const fn name(self) -> &'static str {
        match self {
            Self::Dark => "Night",
            Self::Light => "Light",
            Self::Gray => "Slate",
            Self::Orange => "Amber",
        }
    }

    fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: Color::rgba(10.0 / 255.0, 15.0 / 255.0, 29.0 / 255.0, 1.0),
                card: Color::rgba(24.0 / 255.0, 31.0 / 255.0, 53.0 / 255.0, 1.0),
                border: Color::rgba(78.0 / 255.0, 92.0 / 255.0, 125.0 / 255.0, 1.0),
                display: Color::rgba(16.0 / 255.0, 22.0 / 255.0, 40.0 / 255.0, 1.0),
                button: Color::rgba(47.0 / 255.0, 57.0 / 255.0, 87.0 / 255.0, 1.0),
                subtle: Color::rgba(38.0 / 255.0, 48.0 / 255.0, 76.0 / 255.0, 1.0),
                operator: Color::rgba(105.0 / 255.0, 82.0 / 255.0, 184.0 / 255.0, 1.0),
                accent: Color::rgba(38.0 / 255.0, 192.0 / 255.0, 160.0 / 255.0, 1.0),
                text: Color::rgba(245.0 / 255.0, 247.0 / 255.0, 1.0, 1.0),
                muted: Color::rgba(156.0 / 255.0, 170.0 / 255.0, 205.0 / 255.0, 1.0),
            },
            Self::Light => Palette {
                background: Color::rgba8(235, 239, 247, 255),
                card: Color::rgba8(255, 255, 255, 255),
                border: Color::rgba8(196, 204, 220, 255),
                display: Color::rgba8(244, 247, 252, 255),
                button: Color::rgba8(226, 232, 242, 255),
                subtle: Color::rgba8(214, 221, 234, 255),
                operator: Color::rgba8(127, 104, 205, 255),
                accent: Color::rgba8(22, 158, 130, 255),
                text: Color::rgba8(26, 34, 51, 255),
                muted: Color::rgba8(92, 108, 137, 255),
            },
            Self::Gray => Palette {
                background: Color::rgba8(31, 34, 39, 255),
                card: Color::rgba8(46, 50, 57, 255),
                border: Color::rgba8(92, 98, 108, 255),
                display: Color::rgba8(37, 40, 46, 255),
                button: Color::rgba8(67, 72, 81, 255),
                subtle: Color::rgba8(58, 62, 70, 255),
                operator: Color::rgba8(102, 128, 158, 255),
                accent: Color::rgba8(177, 200, 85, 255),
                text: Color::rgba8(239, 241, 244, 255),
                muted: Color::rgba8(177, 183, 193, 255),
            },
            Self::Orange => Palette {
                background: Color::rgba8(37, 22, 15, 255),
                card: Color::rgba8(63, 38, 25, 255),
                border: Color::rgba8(132, 82, 48, 255),
                display: Color::rgba8(47, 28, 19, 255),
                button: Color::rgba8(91, 54, 34, 255),
                subtle: Color::rgba8(77, 45, 29, 255),
                operator: Color::rgba8(190, 92, 43, 255),
                accent: Color::rgba8(248, 181, 65, 255),
                text: Color::rgba8(255, 243, 229, 255),
                muted: Color::rgba8(220, 178, 142, 255),
            },
        }
    }
}

struct StudioState {
    theme: ThemeChoice,
}
struct StudioTheme(Palette);

impl Theme for StudioTheme {
    fn background(&self) -> Color {
        self.0.background
    }
    fn foreground(&self) -> Color {
        self.0.text
    }
    fn accent(&self) -> Color {
        self.0.accent
    }
    fn control_background(&self) -> Color {
        self.0.button
    }
    fn control_pressed_background(&self) -> Color {
        self.0.subtle
    }
    fn control_hover_background(&self) -> Color {
        self.0.operator
    }
    fn spacing(&self) -> f32 {
        8.0
    }
    fn font_size(&self) -> f32 {
        14.0
    }
    fn corner_radius(&self) -> f32 {
        8.0
    }
}

#[derive(Default)]
struct Metrics {
    frames: u64,
    fps: f64,
    commands: usize,
    paint_micros: u128,
}

fn main() -> Result<(), torn_platform_winit::RunError> {
    torn_platform_winit::run(CalculatorApp::new())
}

struct CalculatorApp {
    runtime: UiRuntime,
    size: Size,
    state: Rc<RefCell<StudioState>>,
    metrics: Rc<RefCell<Metrics>>,
    previous_frame: Option<Instant>,
}

impl CalculatorApp {
    fn new() -> Self {
        let size = valid_size(WINDOW_WIDTH, WINDOW_HEIGHT);
        let state = Rc::new(RefCell::new(StudioState {
            theme: ThemeChoice::Dark,
        }));
        let metrics = Rc::new(RefCell::new(Metrics::default()));
        let layout = Rc::new(RefCell::new(default_workspace()));
        let palette = state.borrow().theme.palette();
        let mut runtime = UiRuntime::with_environment(
            StudioShell {
                state: Rc::clone(&state),
            },
            UiEnvironment::new(StudioTheme(palette)),
        );
        let mut dock_area = DockArea::new(layout);
        for name in ["Calculator", "Metrics", "Appearance"] {
            dock_area
                .register_panel(PanelId::from(name))
                .expect("panel IDs are unique");
        }
        let root = runtime.root();
        let dock = runtime.append_child(root, dock_area).expect("root exists");
        runtime
            .append_child(dock, Calculator::new(Rc::clone(&state)))
            .expect("dock exists");
        runtime
            .append_child(
                dock,
                MetricsPanel {
                    state: Rc::clone(&state),
                    metrics: Rc::clone(&metrics),
                },
            )
            .expect("dock exists");
        runtime
            .append_child(
                dock,
                ThemePanel {
                    state: Rc::clone(&state),
                    size: Size::ZERO,
                },
            )
            .expect("dock exists");
        let mut app = Self {
            runtime,
            size,
            state,
            metrics,
            previous_frame: None,
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

impl WindowApplication for CalculatorApp {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Torn Calculator", self.size)
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
                let palette = self.state.borrow().theme.palette();
                self.runtime
                    .environment_mut()
                    .set_theme(StudioTheme(palette));
                if self.runtime.take_redraw_request() {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
            }
            WindowEvent::Maximized(_)
            | WindowEvent::Fullscreen(_)
            | WindowEvent::RedrawRequested => WindowAction::RequestRedraw,
        }
    }

    fn redraw(&mut self) -> DisplayList {
        let now = Instant::now();
        if let Some(previous_frame) = self.previous_frame.replace(now) {
            let elapsed = now.saturating_duration_since(previous_frame);
            if !elapsed.is_zero() {
                self.metrics.borrow_mut().fps = elapsed.as_secs_f64().recip();
            }
        }
        self.runtime.invalidate_paint();
        let mut display_list = DisplayList::new();
        let _ = self
            .runtime
            .paint(&mut PaintContext::new(&mut display_list));
        let stats = self.runtime.stats();
        let mut metrics = self.metrics.borrow_mut();
        metrics.frames = stats.paint_passes();
        metrics.commands = stats.last_paint_command_count();
        metrics.paint_micros = stats.last_paint_time().as_micros();
        display_list
    }

    fn next_frame_in(&self) -> Option<std::time::Duration> {
        self.runtime.next_frame_in()
    }
}

struct StudioShell {
    state: Rc<RefCell<StudioState>>,
}

impl Widget for StudioShell {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let height = TOP_BAR_HEIGHT.min(size.height());
        let (dock, _) = context
            .layout_child(0, tight(size.width(), (size.height() - height).max(0.0)))
            .expect("studio shell owns one dock area");
        LayoutResult::with_children(
            size,
            vec![torn::ChildLayout::new(dock, Point::new(0.0, height))],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let palette = self.state.borrow().theme.palette();
        let shaper = FontdueTextShaper::ubuntu_light();
        context.fill_rect(bounds, palette.background);
        context.fill_rect(
            Rect::new(
                bounds.origin,
                valid_size(
                    bounds.size.width(),
                    TOP_BAR_HEIGHT.min(bounds.size.height()),
                ),
            ),
            palette.card,
        );
        context.fill_rect(
            Rect::new(
                Point::new(bounds.origin.x, bounds.origin.y + TOP_BAR_HEIGHT - 1.0),
                valid_size(bounds.size.width(), 1.0),
            ),
            palette.border,
        );
        draw_text(
            context,
            &shaper,
            "TORN / CALCULATOR STUDIO",
            14.0,
            palette.text,
            Point::new(bounds.origin.x + 22.0, bounds.origin.y + 19.0),
        );
        let theme = self.state.borrow().theme.name();
        let label = format!("{theme} palette  ·  drag splitters  ·  switch tabs");
        draw_text(
            context,
            &shaper,
            &label,
            12.0,
            palette.muted,
            Point::new(bounds.origin.x + 22.0, bounds.origin.y + 37.0),
        );
    }
}

struct MetricsPanel {
    state: Rc<RefCell<StudioState>>,
    metrics: Rc<RefCell<Metrics>>,
}

impl Widget for MetricsPanel {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(available_size(constraints))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let palette = self.state.borrow().theme.palette();
        let metrics = self.metrics.borrow();
        let shaper = FontdueTextShaper::ubuntu_light();
        context.fill_rect(bounds, palette.background);
        paint_panel_header(
            context,
            &shaper,
            bounds,
            "Runtime metrics",
            "Measured by UiRuntime",
            palette,
        );
        let rows = [
            ("Frames recorded", metrics.frames.to_string()),
            ("Frame rate", format!("{:.1} FPS", metrics.fps)),
            ("Display commands", metrics.commands.to_string()),
            ("Paint recording", format!("{} μs", metrics.paint_micros)),
            ("CPU / GPU / RAM", "not exposed by Torn yet".to_owned()),
        ];
        let mut y = bounds.origin.y + 86.0;
        for (label, value) in rows {
            context.fill_rounded_rect(
                Rect::new(
                    Point::new(bounds.origin.x + 16.0, y),
                    valid_size((bounds.size.width() - 32.0).max(0.0), 42.0),
                ),
                9.0,
                palette.display,
            );
            draw_text(
                context,
                &shaper,
                label,
                11.0,
                palette.muted,
                Point::new(bounds.origin.x + 28.0, y + 8.0),
            );
            draw_text(
                context,
                &shaper,
                &value,
                14.0,
                palette.text,
                Point::new(bounds.origin.x + 28.0, y + 22.0),
            );
            y += 50.0;
        }
        draw_text(
            context,
            &shaper,
            "The unavailable counters are intentionally not fabricated.",
            11.0,
            palette.muted,
            Point::new(bounds.origin.x + 18.0, y + 4.0),
        );
    }
}

struct ThemePanel {
    state: Rc<RefCell<StudioState>>,
    size: Size,
}

impl Widget for ThemePanel {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        self.size = available_size(constraints);
        LayoutResult::new(self.size)
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let palette = self.state.borrow().theme.palette();
        let shaper = FontdueTextShaper::ubuntu_light();
        context.fill_rect(bounds, palette.background);
        paint_panel_header(
            context,
            &shaper,
            bounds,
            "Appearance",
            "Four semantic palettes",
            palette,
        );
        for (index, choice) in ThemeChoice::ALL.iter().enumerate() {
            let rect = theme_choice_rect(self.size, index);
            let choice_palette = choice.palette();
            let selected = matches!(
                (self.state.borrow().theme, choice),
                (ThemeChoice::Dark, ThemeChoice::Dark)
                    | (ThemeChoice::Light, ThemeChoice::Light)
                    | (ThemeChoice::Gray, ThemeChoice::Gray)
                    | (ThemeChoice::Orange, ThemeChoice::Orange)
            );
            context.fill_rounded_rect(offset_rect(rect, bounds.origin), 12.0, choice_palette.card);
            if selected {
                context.stroke_rounded_rect(
                    offset_rect(rect, bounds.origin),
                    12.0,
                    2.0,
                    palette.accent,
                );
            }
            context.fill_rounded_rect(
                Rect::new(
                    Point::new(
                        bounds.origin.x + rect.origin.x + 12.0,
                        bounds.origin.y + rect.origin.y + 13.0,
                    ),
                    valid_size(18.0, 18.0),
                ),
                5.0,
                choice_palette.accent,
            );
            draw_text(
                context,
                &shaper,
                choice.name(),
                14.0,
                choice_palette.text,
                Point::new(
                    bounds.origin.x + rect.origin.x + 40.0,
                    bounds.origin.y + rect.origin.y + 14.0,
                ),
            );
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        let InputEvent::PointerDown(pointer) = event else {
            return EventStatus::Ignored;
        };
        if let Some((_, choice)) = ThemeChoice::ALL
            .iter()
            .enumerate()
            .find(|(index, _)| theme_choice_rect(self.size, *index).contains(pointer.position))
        {
            self.state.borrow_mut().theme = *choice;
            context.request_redraw();
            EventStatus::Handled
        } else {
            EventStatus::Ignored
        }
    }
}

fn default_workspace() -> WorkspaceLayout {
    WorkspaceLayout::new(LayoutNode::split(
        DockAxis::Horizontal,
        0.62,
        LayoutNode::Panel {
            id: PanelId::from("Calculator"),
        },
        LayoutNode::split(
            DockAxis::Vertical,
            0.57,
            LayoutNode::Panel {
                id: PanelId::from("Metrics"),
            },
            LayoutNode::Panel {
                id: PanelId::from("Appearance"),
            },
        ),
    ))
    .expect("static studio layout is valid")
}

fn paint_panel_header(
    context: &mut PaintContext<'_>,
    shaper: &FontdueTextShaper,
    bounds: Rect,
    title: &str,
    subtitle: &str,
    palette: Palette,
) {
    context.fill_rounded_rect(
        Rect::new(
            Point::new(bounds.origin.x + 16.0, bounds.origin.y + 16.0),
            valid_size(34.0, 4.0),
        ),
        2.0,
        palette.accent,
    );
    draw_text(
        context,
        shaper,
        title,
        20.0,
        palette.text,
        Point::new(bounds.origin.x + 16.0, bounds.origin.y + 31.0),
    );
    draw_text(
        context,
        shaper,
        subtitle,
        12.0,
        palette.muted,
        Point::new(bounds.origin.x + 16.0, bounds.origin.y + 58.0),
    );
}

fn draw_text(
    context: &mut PaintContext<'_>,
    shaper: &FontdueTextShaper,
    text: &str,
    size: f32,
    color: Color,
    origin: Point,
) {
    context.draw_text(
        shaper.layout(text, &TextStyle::new(size, color), None),
        origin,
    );
}

fn theme_choice_rect(size: Size, index: usize) -> Rect {
    let width = (size.width() - 48.0).max(0.0);
    Rect::new(
        Point::new(16.0, 84.0 + row_to_f32(index) * 58.0),
        valid_size(width, 48.0),
    )
}

fn offset_rect(rect: Rect, origin: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + origin.x, rect.origin.y + origin.y),
        rect.size,
    )
}

#[derive(Clone, Copy)]
enum CalculatorKey {
    Clear,
    Sign,
    Percent,
    Operator(char),
    Digit(char),
    Decimal,
    Equals,
}

const KEYS: [&[(CalculatorKey, &str)]; 5] = [
    &[
        (CalculatorKey::Clear, "AC"),
        (CalculatorKey::Sign, "±"),
        (CalculatorKey::Percent, "%"),
        (CalculatorKey::Operator('/'), "÷"),
    ],
    &[
        (CalculatorKey::Digit('7'), "7"),
        (CalculatorKey::Digit('8'), "8"),
        (CalculatorKey::Digit('9'), "9"),
        (CalculatorKey::Operator('*'), "×"),
    ],
    &[
        (CalculatorKey::Digit('4'), "4"),
        (CalculatorKey::Digit('5'), "5"),
        (CalculatorKey::Digit('6'), "6"),
        (CalculatorKey::Operator('-'), "−"),
    ],
    &[
        (CalculatorKey::Digit('1'), "1"),
        (CalculatorKey::Digit('2'), "2"),
        (CalculatorKey::Digit('3'), "3"),
        (CalculatorKey::Operator('+'), "+"),
    ],
    &[
        (CalculatorKey::Digit('0'), "0"),
        (CalculatorKey::Decimal, "."),
        (CalculatorKey::Equals, "="),
    ],
];

struct Calculator {
    state: Rc<RefCell<StudioState>>,
    display: String,
    expression: String,
    stored: Option<f64>,
    pending: Option<char>,
    fresh: bool,
    error: bool,
    size: Size,
}

impl Calculator {
    fn new(state: Rc<RefCell<StudioState>>) -> Self {
        Self {
            state,
            display: "0".to_owned(),
            expression: "READY".to_owned(),
            stored: None,
            pending: None,
            fresh: true,
            error: false,
            size: Size::ZERO,
        }
    }

    fn apply(&mut self, key: CalculatorKey) {
        match key {
            CalculatorKey::Clear => self.clear(),
            CalculatorKey::Sign => {
                if !self.fresh && self.display != "0" {
                    if let Some(stripped) = self.display.strip_prefix('-') {
                        self.display = stripped.to_owned();
                    } else {
                        self.display.insert(0, '-');
                    }
                }
            }
            CalculatorKey::Percent => {
                if let Some(value) = self.value() {
                    self.display = format_number(value / 100.0);
                    self.fresh = true;
                }
            }
            CalculatorKey::Digit(digit) => self.push_digit(digit),
            CalculatorKey::Decimal => self.push_decimal(),
            CalculatorKey::Operator(operator) => self.set_operator(operator),
            CalculatorKey::Equals => self.equals(),
        }
    }

    fn clear(&mut self) {
        "0".clone_into(&mut self.display);
        "READY".clone_into(&mut self.expression);
        self.stored = None;
        self.pending = None;
        self.fresh = true;
        self.error = false;
    }

    fn push_digit(&mut self, digit: char) {
        if self.fresh || self.error {
            self.display.clear();
            self.error = false;
        }
        if self.display == "0" {
            self.display.clear();
        }
        if self.display.len() < 15 {
            self.display.push(digit);
        }
        self.fresh = false;
    }

    fn push_decimal(&mut self) {
        if self.fresh || self.error {
            "0".clone_into(&mut self.display);
            self.error = false;
        }
        if !self.display.contains('.') {
            self.display.push('.');
        }
        self.fresh = false;
    }

    fn set_operator(&mut self, operator: char) {
        let Some(current) = self.value() else {
            return;
        };
        if let (Some(stored), Some(pending)) = (self.stored, self.pending)
            && !self.fresh
        {
            if let Some(result) = calculate(stored, current, pending) {
                self.display = format_number(result);
            } else {
                self.show_error();
                return;
            }
        }
        self.stored = self.value();
        self.pending = Some(operator);
        self.expression = format!("{} {}", self.display, operator_label(operator));
        self.fresh = true;
    }

    fn equals(&mut self) {
        let (Some(stored), Some(operator), Some(current)) =
            (self.stored, self.pending, self.value())
        else {
            return;
        };
        match calculate(stored, current, operator) {
            Some(result) => {
                self.expression = format!(
                    "{} {} {} =",
                    format_number(stored),
                    operator_label(operator),
                    format_number(current)
                );
                self.display = format_number(result);
                self.stored = None;
                self.pending = None;
                self.fresh = true;
            }
            None => self.show_error(),
        }
    }

    fn show_error(&mut self) {
        "Error".clone_into(&mut self.display);
        "DIVISION BY ZERO".clone_into(&mut self.expression);
        self.stored = None;
        self.pending = None;
        self.fresh = true;
        self.error = true;
    }

    fn value(&self) -> Option<f64> {
        (!self.error)
            .then(|| self.display.parse::<f64>().ok())
            .flatten()
    }

    fn key_at(bounds: Rect, point: Point) -> Option<CalculatorKey> {
        let layout = CalculatorLayout::new(bounds);
        KEYS.iter().enumerate().find_map(|(row, keys)| {
            keys.iter().enumerate().find_map(|(column, (key, _))| {
                layout
                    .button_rect(row, column)
                    .contains(point)
                    .then_some(*key)
            })
        })
    }
}

impl Widget for Calculator {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        self.size = available_size(constraints);
        LayoutResult::new(self.size)
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let layout = CalculatorLayout::new(bounds);
        let shaper = FontdueTextShaper::ubuntu_light();
        let palette = self.state.borrow().theme.palette();

        context.fill_rect(bounds, palette.background);
        context.fill_rounded_rect(layout.card, 30.0, palette.card);
        context.stroke_rounded_rect(layout.card, 30.0, 1.0, palette.border);
        context.fill_rounded_rect(layout.display, 22.0, palette.display);
        context.fill_rounded_rect(
            Rect::new(
                Point::new(layout.card.origin.x + 28.0, layout.card.origin.y + 28.0),
                valid_size(38.0, 4.0),
            ),
            2.0,
            palette.accent,
        );

        draw_right_aligned(
            context,
            &shaper,
            &self.expression,
            12.0,
            palette.muted,
            layout.display,
            28.0,
        );
        draw_right_aligned(
            context,
            &shaper,
            &self.display,
            display_font_size(&self.display),
            palette.text,
            layout.display,
            68.0,
        );

        for (row, keys) in KEYS.iter().enumerate() {
            for (column, (key, label)) in keys.iter().enumerate() {
                let rect = layout.button_rect(row, column);
                let color = match key {
                    CalculatorKey::Clear | CalculatorKey::Sign | CalculatorKey::Percent => {
                        palette.subtle
                    }
                    CalculatorKey::Operator(_) => palette.operator,
                    CalculatorKey::Equals => palette.accent,
                    _ => palette.button,
                };
                context.fill_rounded_rect(rect, 16.0, color);
                let text_color = if matches!(key, CalculatorKey::Equals) {
                    palette.background
                } else {
                    palette.text
                };
                let text = shaper.layout(label, &TextStyle::new(24.0, text_color), None);
                context.draw_text(
                    text.clone(),
                    Point::new(
                        rect.origin.x + (rect.size.width() - text.size().width()) / 2.0,
                        rect.origin.y + (rect.size.height() - text.size().height()) / 2.0 - 1.0,
                    ),
                );
            }
        }

        let hint = shaper.layout(
            "Keyboard ready  •  Esc clears",
            &TextStyle::new(11.0, palette.muted),
            None,
        );
        context.draw_text(
            hint.clone(),
            Point::new(
                layout.card.origin.x + (layout.card.size.width() - hint.size().width()) / 2.0,
                layout.card.origin.y + layout.card.size.height() - 22.0,
            ),
        );
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        let key = match event {
            InputEvent::PointerDown(pointer) => {
                context.request_focus();
                Self::key_at(Rect::new(Point::ZERO, self.size), pointer.position)
            }
            InputEvent::TextInput(text) => text.chars().find_map(key_from_character),
            InputEvent::KeyDown(event) => match event.key {
                Key::Named(NamedKey::Backspace) => {
                    if !self.fresh && !self.error {
                        self.display.pop();
                        if self.display.is_empty() || self.display == "-" {
                            "0".clone_into(&mut self.display);
                            self.fresh = true;
                        }
                    }
                    context.request_redraw();
                    return EventStatus::Handled;
                }
                Key::Named(NamedKey::Enter) => Some(CalculatorKey::Equals),
                Key::Named(NamedKey::Escape) => Some(CalculatorKey::Clear),
                _ => None,
            },
            _ => None,
        };
        if let Some(key) = key {
            self.apply(key);
            context.request_redraw();
            EventStatus::Handled
        } else {
            EventStatus::Ignored
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

struct CalculatorLayout {
    card: Rect,
    display: Rect,
    keypad_origin: Point,
    button_width: f32,
    button_height: f32,
    gap: f32,
}

impl CalculatorLayout {
    fn new(bounds: Rect) -> Self {
        let margin = (bounds.size.width().min(bounds.size.height()) * 0.055).clamp(16.0, 28.0);
        let width = (bounds.size.width() - margin * 2.0).clamp(0.0, 404.0);
        let height = (bounds.size.height() - margin * 2.0).clamp(0.0, 664.0);
        let card = Rect::new(
            Point::new(
                bounds.origin.x + (bounds.size.width() - width) / 2.0,
                bounds.origin.y + (bounds.size.height() - height) / 2.0,
            ),
            valid_size(width, height),
        );
        let inset = (width * 0.065).clamp(16.0, 26.0);
        let display = Rect::new(
            Point::new(card.origin.x + inset, card.origin.y + 52.0),
            valid_size(
                (width - inset * 2.0).max(0.0),
                (height * 0.22).clamp(112.0, 146.0),
            ),
        );
        let gap = (width * 0.035).clamp(8.0, 12.0);
        let button_width = ((display.size.width() - gap * 3.0) / 4.0).max(0.0);
        let button_height = ((card.origin.y + card.size.height()
            - 48.0
            - (display.origin.y + display.size.height())
            - gap * 5.0)
            / 5.0)
            .max(0.0);
        Self {
            card,
            display,
            keypad_origin: Point::new(
                display.origin.x,
                display.origin.y + display.size.height() + gap * 1.5,
            ),
            button_width,
            button_height,
            gap,
        }
    }

    fn button_rect(&self, row: usize, column: usize) -> Rect {
        let grid_column = if row == 4 && column > 0 {
            column + 1
        } else {
            column
        };
        let width = if row == 4 && column == 0 {
            self.button_width * 2.0 + self.gap
        } else {
            self.button_width
        };
        Rect::new(
            Point::new(
                self.keypad_origin.x + (self.button_width + self.gap) * row_to_f32(grid_column),
                self.keypad_origin.y + (self.button_height + self.gap) * row_to_f32(row),
            ),
            valid_size(width, self.button_height),
        )
    }
}

fn key_from_character(character: char) -> Option<CalculatorKey> {
    match character {
        '0'..='9' => Some(CalculatorKey::Digit(character)),
        '.' | ',' => Some(CalculatorKey::Decimal),
        '+' | '-' | '*' | '/' => Some(CalculatorKey::Operator(character)),
        '=' => Some(CalculatorKey::Equals),
        '%' => Some(CalculatorKey::Percent),
        _ => None,
    }
}

fn calculate(left: f64, right: f64, operator: char) -> Option<f64> {
    let value = match operator {
        '+' => left + right,
        '-' => left - right,
        '*' => left * right,
        '/' if right != 0.0 => left / right,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn operator_label(operator: char) -> char {
    match operator {
        '*' => '×',
        '/' => '÷',
        _ => operator,
    }
}

fn format_number(value: f64) -> String {
    let rounded = value.round();
    if (value - rounded).abs() < 1e-10 {
        format!("{rounded:.0}")
    } else {
        format!("{value:.10}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

fn draw_right_aligned(
    context: &mut PaintContext<'_>,
    shaper: &FontdueTextShaper,
    text: &str,
    font_size: f32,
    color: Color,
    bounds: Rect,
    y: f32,
) {
    let layout = shaper.layout(text, &TextStyle::new(font_size, color), None);
    context.draw_text(
        layout.clone(),
        Point::new(
            bounds.origin.x + bounds.size.width() - layout.size().width() - 22.0,
            bounds.origin.y + y,
        ),
    );
}

fn display_font_size(value: &str) -> f32 {
    match value.chars().count() {
        0..=8 => 48.0,
        9..=11 => 39.0,
        _ => 31.0,
    }
}

fn available_size(constraints: Constraints) -> Size {
    let maximum = constraints.max();
    let minimum = constraints.min();
    constraints.constrain(valid_size(
        if maximum.width().is_finite() {
            maximum.width()
        } else {
            minimum.width()
        },
        if maximum.height().is_finite() {
            maximum.height()
        } else {
            minimum.height()
        },
    ))
}

fn tight(width: f32, height: f32) -> Constraints {
    Constraints::tight(valid_size(width, height)).expect("calculator constraints are valid")
}

fn valid_size(width: f32, height: f32) -> Size {
    Size::new(width.max(0.0), height.max(0.0))
        .expect("calculator dimensions are finite and non-negative")
}

fn row_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}
