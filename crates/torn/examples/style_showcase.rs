//! Interactive component laboratory for all standard Torn widgets.
//!
//! Run with `cargo run -p torn --example style_showcase`.
//! The showcase intentionally uses only the public `torn` facade.

use std::{cell::RefCell, rc::Rc};

use torn::{
    Align, Alignment, Border, Button, Card, Checkbox, ChildLayout, Color, Column, Constraints,
    DockArea, DockAxis, Dropdown, Image, Insets, LayoutContext, LayoutNode, LayoutResult, ListView,
    MenuBar, PanelId, Point, ProgressBar, RadioButton, RadioGroup, Rect, Row, ScrollView, Size,
    SizedBox, Slider, SplitAxis, Splitter, Stack, Subscription, TabBar, Text, TextField, Theme,
    Toolbar, UiEnvironment, UiRuntime, Widget, WorkspaceLayout,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, FontdueTextShaper, PaintContext, RasterImage, TextStyle},
};

const WINDOW_WIDTH: f32 = 1_280.0;
const WINDOW_HEIGHT: f32 = 820.0;
const MENU_HEIGHT: f32 = 28.0;
const TOOLBAR_HEIGHT: f32 = 44.0;
const PAGE_PADDING: f32 = 24.0;
const PANEL_GAP: f32 = 16.0;

const CANVAS: Color = Color::rgba(11.0 / 255.0, 16.0 / 255.0, 29.0 / 255.0, 1.0);
const SURFACE: Color = Color::rgba(23.0 / 255.0, 33.0 / 255.0, 58.0 / 255.0, 1.0);
const SURFACE_BORDER: Color = Color::rgba(59.0 / 255.0, 78.0 / 255.0, 120.0 / 255.0, 1.0);
const CARD: Color = Color::rgba(18.0 / 255.0, 26.0 / 255.0, 47.0 / 255.0, 1.0);
const CARD_BORDER: Color = Color::rgba(47.0 / 255.0, 64.0 / 255.0, 101.0 / 255.0, 1.0);
const TEXT: Color = Color::rgba(242.0 / 255.0, 245.0 / 255.0, 1.0, 1.0);
const MUTED: Color = Color::rgba(164.0 / 255.0, 178.0 / 255.0, 207.0 / 255.0, 1.0);
const TEAL: Color = Color::rgba(46.0 / 255.0, 220.0 / 255.0, 179.0 / 255.0, 1.0);
const LILAC: Color = Color::rgba(166.0 / 255.0, 135.0 / 255.0, 1.0, 1.0);
const ORANGE: Color = Color::rgba(1.0, 184.0 / 255.0, 92.0 / 255.0, 1.0);
const CONTROL: Color = Color::rgba(28.0 / 255.0, 39.0 / 255.0, 68.0 / 255.0, 1.0);
const CONTROL_HOVER: Color = Color::rgba(41.0 / 255.0, 57.0 / 255.0, 98.0 / 255.0, 1.0);
const CONTROL_PRESSED: Color = Color::rgba(53.0 / 255.0, 73.0 / 255.0, 124.0 / 255.0, 1.0);

/// Keeps standard widgets in the same deep-navy palette as the hand-painted chrome.
struct ShowcaseTheme;

impl Theme for ShowcaseTheme {
    fn background(&self) -> Color {
        CANVAS
    }

    fn surface(&self) -> Color {
        SURFACE
    }

    fn raised_surface(&self) -> Color {
        CARD
    }

    fn foreground(&self) -> Color {
        TEXT
    }

    fn accent(&self) -> Color {
        TEAL
    }

    fn accent_foreground(&self) -> Color {
        CANVAS
    }

    fn control_background(&self) -> Color {
        CONTROL
    }

    fn control_hover_background(&self) -> Color {
        CONTROL_HOVER
    }

    fn control_pressed_background(&self) -> Color {
        CONTROL_PRESSED
    }

    fn border(&self) -> Color {
        CARD_BORDER
    }

    fn focus_ring(&self) -> Color {
        TEAL
    }

    fn shadow(&self) -> Color {
        Color::rgba(0.0, 0.0, 0.0, 0.55)
    }

    fn muted_foreground(&self) -> Color {
        MUTED
    }

    fn title_bar_background(&self) -> Color {
        SURFACE
    }

    fn title_bar_hover_background(&self) -> Color {
        CONTROL_HOVER
    }

    fn control_padding(&self) -> Insets {
        Insets::new(7.0, 14.0, 7.0, 14.0)
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

fn main() -> Result<(), torn_platform_winit::RunError> {
    torn_platform_winit::run(StyleShowcase::new())
}

struct StyleShowcase {
    runtime: UiRuntime,
    size: Size,
    _subscriptions: Vec<Subscription>,
}

impl StyleShowcase {
    fn new() -> Self {
        let size = valid_size(WINDOW_WIDTH, WINDOW_HEIGHT);
        let mut runtime =
            UiRuntime::with_environment(ShowcaseShell, UiEnvironment::new(ShowcaseTheme));
        let root = runtime.root();
        let mut subscriptions = Vec::new();

        let menu = MenuBar::new(["File", "Edit", "View", "Help"]);
        subscriptions.push(menu.activated().subscribe(|index| {
            let name = ["File", "Edit", "View", "Help"][*index];
            println!("MenuBar: открыт раздел {name}.");
        }));
        runtime
            .append_child(root, menu)
            .expect("showcase root exists");

        let toolbar = runtime
            .append_child(root, Toolbar::new().with_padding(Insets::all(6.0)))
            .expect("showcase root exists");
        for (icon, action) in [
            ("+", "Создан новый документ."),
            ("S", "Состояние сохранено."),
            ("↶", "Отмена действия."),
            ("⚙", "Открыты настройки."),
        ] {
            append_toolbar_button(&mut runtime, &mut subscriptions, toolbar, icon, action);
        }

        let tabs = TabBar::new(["Компоненты", "Компоновка", "Рабочая область"]);
        subscriptions.push(
            tabs.changed()
                .subscribe(|index| println!("TabBar: открыта страница {}.", index + 1)),
        );
        let tabs = runtime
            .append_child(root, tabs)
            .expect("showcase root exists");
        append_components_page(&mut runtime, &mut subscriptions, tabs);
        append_layout_page(&mut runtime, tabs);
        append_workspace_page(&mut runtime, tabs);

        let mut app = Self {
            runtime,
            size,
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
        WindowOptions::new("Torn — Component Laboratory", self.size)
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
                if self.runtime.take_redraw_request() {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
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

    fn next_frame_in(&self) -> Option<std::time::Duration> {
        self.runtime.next_frame_in()
    }
}

/// The application chrome: menu bar, quick actions, and a tabbed content area.
struct ShowcaseShell;

impl Widget for ShowcaseShell {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let menu_height = MENU_HEIGHT.min(size.height());
        let toolbar_height = TOOLBAR_HEIGHT.min((size.height() - menu_height).max(0.0));
        let tabs_height = (size.height() - menu_height - toolbar_height).max(0.0);
        let menu = layout_child(context, 0, size.width(), menu_height);
        let toolbar = layout_child(context, 1, size.width(), toolbar_height);
        let tabs = layout_child(context, 2, size.width(), tabs_height);
        LayoutResult::with_children(
            size,
            vec![
                ChildLayout::new(menu, Point::ZERO),
                ChildLayout::new(toolbar, Point::new(0.0, menu_height)),
                ChildLayout::new(tabs, Point::new(0.0, menu_height + toolbar_height)),
            ],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, CANVAS);
    }
}

/// Arranges the component controls in two responsive columns.
struct ComponentsPage;

impl Widget for ComponentsPage {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let inset = PAGE_PADDING
            .min(size.width() / 12.0)
            .min(size.height() / 12.0);
        let content_width = (size.width() - inset * 2.0).max(0.0);
        let content_height = (size.height() - inset * 2.0).max(0.0);
        let left_width = (content_width * 0.58 - PANEL_GAP / 2.0).max(0.0);
        let right_width = (content_width - left_width - PANEL_GAP).max(0.0);
        let controls_height = (content_height * 0.57).max(0.0);
        let gallery_height = (content_height - controls_height - PANEL_GAP).max(0.0);
        let controls = layout_child(context, 0, left_width, controls_height);
        let gallery = layout_child(context, 1, left_width, gallery_height);
        let list = layout_child(context, 2, right_width, content_height);
        LayoutResult::with_children(
            size,
            vec![
                ChildLayout::new(controls, Point::new(inset, inset)),
                ChildLayout::new(
                    gallery,
                    Point::new(inset, inset + controls_height + PANEL_GAP),
                ),
                ChildLayout::new(list, Point::new(inset + left_width + PANEL_GAP, inset)),
            ],
        )
    }
}

/// Places a varied set of input controls inside the first components card.
struct ControlsPanel;

impl Widget for ControlsPanel {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let header_height = 62.0_f32.min(size.height());
        let row_height = 34.0_f32.min((size.height() - header_height).max(0.0));
        let left_width = (size.width() * 0.49).max(0.0);
        let right_x = left_width + 14.0;
        let right_width = (size.width() - right_x).max(0.0);
        let mut children = Vec::with_capacity(context.child_count());
        for index in 0..context.child_count() {
            let (width, height, origin) = match index {
                0 => (size.width(), row_height, Point::new(0.0, header_height)),
                1 => (
                    left_width,
                    row_height,
                    Point::new(0.0, header_height + row_height + 8.0),
                ),
                2 => (
                    left_width,
                    row_height,
                    Point::new(0.0, header_height + row_height * 2.0 + 16.0),
                ),
                3 => (
                    left_width,
                    row_height,
                    Point::new(0.0, header_height + row_height * 3.0 + 24.0),
                ),
                4 => (
                    right_width,
                    row_height,
                    Point::new(right_x, header_height + row_height + 8.0),
                ),
                5 => (
                    right_width,
                    row_height,
                    Point::new(right_x, header_height + row_height * 2.0 + 16.0),
                ),
                6 => (
                    right_width,
                    14.0,
                    Point::new(right_x, header_height + row_height * 3.0 + 29.0),
                ),
                _ => (0.0, 0.0, Point::ZERO),
            };
            let id = layout_child(context, index, width, height);
            children.push(ChildLayout::new(id, origin));
        }
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        paint_card(
            context,
            bounds,
            "Управление",
            "Все элементы отвечают на ввод, фокус и события.",
            TEAL,
        );
        let shaper = FontdueTextShaper::ubuntu_light();
        context.draw_text(
            shaper.layout("Переключатели", &TextStyle::new(12.0, MUTED), None),
            Point::new(bounds.origin.x, bounds.origin.y + 102.0),
        );
        context.draw_text(
            shaper.layout("Ввод и выбор", &TextStyle::new(12.0, MUTED), None),
            Point::new(
                bounds.origin.x + bounds.size.width() * 0.49 + 14.0,
                bounds.origin.y + 102.0,
            ),
        );
    }
}

/// Shows `Image`, `ProgressBar`, and styled surfaces side by side.
struct GalleryPanel;

impl Widget for GalleryPanel {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let image = layout_child(
            context,
            0,
            74.0_f32.min(size.width()),
            74.0_f32.min(size.height()),
        );
        let progress_width = (size.width() - 104.0).max(0.0);
        let progress = layout_child(context, 1, progress_width, 14.0_f32.min(size.height()));
        LayoutResult::with_children(
            size,
            vec![
                ChildLayout::new(image, Point::new(0.0, 62.0)),
                ChildLayout::new(progress, Point::new(92.0, 92.0)),
            ],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        paint_card(
            context,
            bounds,
            "Растр и статус",
            "Image масштабируется, ProgressBar отражает состояние.",
            LILAC,
        );
        let shaper = FontdueTextShaper::ubuntu_light();
        context.draw_text(
            shaper.layout("Render pipeline 72%", &TextStyle::new(13.0, TEXT), None),
            Point::new(bounds.origin.x + 92.0, bounds.origin.y + 62.0),
        );
    }
}

/// A simple vertical column larger than its viewport, used by `ScrollView`.
struct LongDocument;

impl Widget for LongDocument {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let width = constraints.max().width();
        LayoutResult::new(constraints.constrain(valid_size(width, 680.0)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        paint_card(
            context,
            bounds,
            "ScrollView",
            "Прокручивайте этот журнал колёсиком мыши.",
            ORANGE,
        );
        let shaper = FontdueTextShaper::ubuntu_light();
        for index in 0..12 {
            let y = bounds.origin.y + 76.0 + usize_to_f32(index) * 46.0;
            context.fill_rounded_rect(
                Rect::new(
                    Point::new(bounds.origin.x, y),
                    valid_size(bounds.size.width(), 32.0),
                ),
                8.0,
                if index % 2 == 0 { SURFACE } else { CARD },
            );
            let text = format!(
                "Событие {:02}  ·  retained tree · display list · input routing",
                index + 1
            );
            context.draw_text(
                shaper.layout(
                    &text,
                    &TextStyle::new(13.0, TEXT),
                    Some(bounds.size.width() - 20.0),
                ),
                Point::new(bounds.origin.x + 10.0, y + 9.0),
            );
        }
    }
}

/// Positions a draggable splitter and an independently virtualized list.
struct LayoutPage;

impl Widget for LayoutPage {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let inset = PAGE_PADDING
            .min(size.width() / 12.0)
            .min(size.height() / 12.0);
        let content_width = (size.width() - inset * 2.0).max(0.0);
        let content_height = (size.height() - inset * 2.0).max(0.0);
        let split_height = (content_height * 0.48).max(0.0);
        let splitter = layout_child(context, 0, content_width, split_height);
        let list = layout_child(
            context,
            1,
            content_width,
            (content_height - split_height - PANEL_GAP).max(0.0),
        );
        LayoutResult::with_children(
            size,
            vec![
                ChildLayout::new(splitter, Point::new(inset, inset)),
                ChildLayout::new(list, Point::new(inset, inset + split_height + PANEL_GAP)),
            ],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, CANVAS);
    }
}

/// The first splitter pane.
struct PaneInfo {
    title: &'static str,
    accent: Color,
}

impl Widget for PaneInfo {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let maximum = constraints.max();
        let width = if maximum.width().is_finite() {
            maximum.width()
        } else {
            260.0
        };
        let height = if maximum.height().is_finite() {
            maximum.height()
        } else {
            160.0
        };
        LayoutResult::new(constraints.constrain(valid_size(width, height)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, SURFACE);
        context.stroke_rect(bounds, 1.0, SURFACE_BORDER);
        let shaper = FontdueTextShaper::ubuntu_light();
        context.fill_rounded_rect(
            Rect::new(
                Point::new(bounds.origin.x + 18.0, bounds.origin.y + 18.0),
                valid_size(54.0, 5.0),
            ),
            2.5,
            self.accent,
        );
        context.draw_text(
            shaper.layout(
                self.title,
                &TextStyle::new(20.0, TEXT),
                Some(bounds.size.width() - 36.0),
            ),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 42.0),
        );
        context.draw_text(
            shaper.layout(
                "Перетащите разделитель: обе дочерние панели получают новые строгие constraints.",
                &TextStyle::new(14.0, MUTED),
                Some((bounds.size.width() - 36.0).max(0.0)),
            ),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 80.0),
        );
    }
}

/// Rows painted by `ListView`.
struct ListRow {
    index: usize,
}

impl Widget for ListRow {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(available_size(constraints))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let color = if self.index.is_multiple_of(2) {
            SURFACE
        } else {
            CARD
        };
        context.fill_rect(bounds, color);
        let shaper = FontdueTextShaper::ubuntu_light();
        let label = format!(
            "{:02}    виртуализированная строка ListView",
            self.index + 1
        );
        context.draw_text(
            shaper.layout(&label, &TextStyle::new(14.0, TEXT), None),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 10.0),
        );
    }
}

/// Hosts a live `DockArea` with a persisted `WorkspaceLayout` model.
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
        let dock = layout_child(
            context,
            0,
            (size.width() - inset * 2.0).max(0.0),
            (size.height() - inset * 2.0).max(0.0),
        );
        LayoutResult::with_children(size, vec![ChildLayout::new(dock, Point::new(inset, inset))])
    }
}

struct WorkspacePanel {
    title: &'static str,
    body: &'static str,
    color: Color,
}

impl Widget for WorkspacePanel {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(available_size(constraints))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, self.color);
        let shaper = FontdueTextShaper::ubuntu_light();
        context.draw_text(
            shaper.layout(self.title, &TextStyle::new(19.0, TEXT), None),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 18.0),
        );
        context.draw_text(
            shaper.layout(
                self.body,
                &TextStyle::new(14.0, MUTED),
                Some(bounds.size.width() - 36.0),
            ),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 58.0),
        );
    }
}

fn append_components_page(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    tabs: torn::WidgetId,
) {
    let page = runtime
        .append_child(tabs, ComponentsPage)
        .expect("tab bar exists");
    let controls = runtime
        .append_child(
            page,
            Card::new()
                .with_background(CARD)
                .with_corner_radius(16.0)
                .with_border(Border::new(1.0, CARD_BORDER))
                .with_elevation(10.0)
                .with_padding(Insets::all(18.0)),
        )
        .expect("components page exists");
    let controls = runtime
        .append_child(controls, ControlsPanel)
        .expect("controls surface exists");
    append_controls(runtime, subscriptions, controls);

    let gallery = runtime
        .append_child(page, GalleryPanel)
        .expect("components page exists");
    let logo_stack = runtime
        .append_child(gallery, Stack::new())
        .expect("gallery exists");
    let logo_align = runtime
        .append_child(logo_stack, Align::new(Alignment::Center, Alignment::Center))
        .expect("image stack exists");
    let logo_size = runtime
        .append_child(logo_align, SizedBox::new(Some(74.0), Some(74.0)))
        .expect("image alignment exists");
    runtime
        .append_child(logo_size, Image::with_size(torn_logo(), 74.0, 74.0))
        .expect("image size box exists");
    runtime
        .append_child(gallery, ProgressBar::with_progress(0.72))
        .expect("gallery exists");

    let scroll = runtime
        .append_child(page, ScrollView::vertical())
        .expect("components page exists");
    let document = runtime
        .append_child(scroll, Column::new())
        .expect("scroll view exists");
    runtime
        .append_child(document, LongDocument)
        .expect("document column exists");
}

fn append_controls(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    parent: torn::WidgetId,
) {
    let text_field = TextField::with_text("Наберите что-нибудь");
    subscriptions.push(
        text_field
            .changed()
            .subscribe(|text| println!("TextField: {text}")),
    );
    runtime
        .append_child(parent, text_field)
        .expect("controls panel exists");

    let checkbox = Checkbox::with_checked(true);
    subscriptions.push(
        checkbox
            .toggled()
            .subscribe(|checked| println!("Checkbox: {checked}")),
    );
    append_labeled(runtime, parent, checkbox, "Включить анимации");

    let group = RadioGroup::new();
    group.set_selected(&"Темная");
    subscriptions.push(
        group
            .changed()
            .subscribe(|theme| println!("RadioGroup: {theme}")),
    );
    append_labeled(
        runtime,
        parent,
        RadioButton::new(group.clone(), "Темная"),
        "Тёмная тема",
    );
    append_labeled(
        runtime,
        parent,
        RadioButton::new(group, "Светлая"),
        "Светлая тема",
    );

    let dropdown = Dropdown::new(["Авто", "Компактно", "Просторно"]);
    subscriptions.push(
        dropdown
            .changed()
            .subscribe(|index| println!("Dropdown: выбран вариант {index}")),
    );
    runtime
        .append_child(parent, dropdown)
        .expect("controls panel exists");

    let slider = Slider::with_value(0.0, 100.0, 64.0);
    subscriptions.push(
        slider
            .changed()
            .subscribe(|value| println!("Slider: {value:.0}")),
    );
    runtime
        .append_child(parent, slider)
        .expect("controls panel exists");
    runtime
        .append_child(parent, ProgressBar::with_progress(0.64))
        .expect("controls panel exists");
}

fn append_layout_page(runtime: &mut UiRuntime, tabs: torn::WidgetId) {
    let page = runtime
        .append_child(tabs, LayoutPage)
        .expect("tab bar exists");
    let splitter = runtime
        .append_child(page, Splitter::new(SplitAxis::Horizontal, 0.52))
        .expect("layout page exists");
    let first_pane = runtime
        .append_child(splitter, Row::new())
        .expect("splitter exists");
    runtime
        .append_child(
            first_pane,
            PaneInfo {
                title: "Splitter",
                accent: TEAL,
            },
        )
        .expect("splitter row exists");
    runtime
        .append_child(
            splitter,
            PaneInfo {
                title: "Runtime children",
                accent: LILAC,
            },
        )
        .expect("splitter exists");

    let list = runtime
        .append_child(page, ListView::new(40.0))
        .expect("layout page exists");
    for index in 0..40 {
        runtime
            .append_child(list, ListRow { index })
            .expect("list view exists");
    }
}

fn append_workspace_page(runtime: &mut UiRuntime, tabs: torn::WidgetId) {
    let page = runtime
        .append_child(tabs, WorkspacePage)
        .expect("tab bar exists");
    let layout = Rc::new(RefCell::new(
        WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Horizontal,
            0.24,
            LayoutNode::Panel {
                id: PanelId::from("Навигатор"),
            },
            LayoutNode::tabs(vec![PanelId::from("Сцена"), PanelId::from("Инспектор")]),
        ))
        .expect("showcase workspace layout is valid"),
    ));
    let mut dock = DockArea::new(layout);
    dock.register_panel(PanelId::from("Навигатор"))
        .expect("panel is unique");
    dock.register_panel(PanelId::from("Сцена"))
        .expect("panel is unique");
    dock.register_panel(PanelId::from("Инспектор"))
        .expect("panel is unique");
    let dock = runtime
        .append_child(page, dock)
        .expect("workspace page exists");
    for panel in [
        WorkspacePanel {
            title: "Навигатор",
            body: "DockArea проецирует устойчивую модель WorkspaceLayout на retained children.",
            color: Color::rgba8(26, 47, 53, 255),
        },
        WorkspacePanel {
            title: "Сцена",
            body: "Переключайте вкладки и перетаскивайте разделитель между группами.",
            color: Color::rgba8(47, 39, 71, 255),
        },
        WorkspacePanel {
            title: "Инспектор",
            body: "DockArea хранит UI отдельно от сериализуемой модели рабочих панелей.",
            color: Color::rgba8(65, 47, 36, 255),
        },
    ] {
        runtime.append_child(dock, panel).expect("dock area exists");
    }
}

fn append_toolbar_button(
    runtime: &mut UiRuntime,
    subscriptions: &mut Vec<Subscription>,
    toolbar: torn::WidgetId,
    icon: &'static str,
    action: &'static str,
) {
    let button = Button::new()
        .with_background(SURFACE)
        .with_hover_background(Color::rgba8(44, 61, 100, 255))
        .with_pressed_background(Color::rgba8(32, 45, 75, 255))
        .with_corner_radius(7.0)
        .with_border(Border::new(1.0, CARD_BORDER))
        .with_padding(Insets::all(7.0));
    subscriptions.push(button.activated().subscribe(move |()| println!("{action}")));
    let button = runtime
        .append_child(toolbar, button)
        .expect("toolbar exists");
    runtime
        .append_child(button, label(icon, 15.0, TEXT))
        .expect("toolbar button exists");
}

fn append_labeled<W>(runtime: &mut UiRuntime, parent: torn::WidgetId, widget: W, text: &str)
where
    W: Widget + 'static,
{
    let widget = runtime
        .append_child(parent, widget)
        .expect("controls panel exists");
    runtime
        .append_child(widget, label(text, 14.0, TEXT))
        .expect("labeled control exists");
}

fn paint_card(
    context: &mut PaintContext<'_>,
    bounds: Rect,
    title: &str,
    description: &str,
    accent: Color,
) {
    context.draw_shadow(bounds, 16.0, 10.0, Color::rgba(0.0, 0.0, 0.0, 0.45));
    context.fill_rounded_rect(bounds, 16.0, CARD);
    context.stroke_rounded_rect(bounds, 16.0, 1.0, CARD_BORDER);
    context.fill_rounded_rect(
        Rect::new(
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 19.0),
            valid_size(42.0, 5.0),
        ),
        2.5,
        accent,
    );
    let shaper = FontdueTextShaper::ubuntu_light();
    context.draw_text(
        shaper.layout(
            title,
            &TextStyle::new(20.0, TEXT),
            Some(bounds.size.width() - 36.0),
        ),
        Point::new(bounds.origin.x + 18.0, bounds.origin.y + 34.0),
    );
    context.draw_text(
        shaper.layout(
            description,
            &TextStyle::new(13.0, MUTED),
            Some((bounds.size.width() - 36.0).max(0.0)),
        ),
        Point::new(bounds.origin.x + 18.0, bounds.origin.y + 62.0),
    );
}

fn torn_logo() -> RasterImage {
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let color = if x == 7 || y == 7 || (x > 3 && x < 12 && y > 3 && y < 12) {
                TEAL
            } else if (x + y) % 3 == 0 {
                LILAC
            } else {
                Color::rgba8(25, 38, 69, 255)
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

fn label(text: &str, size: f32, color: Color) -> Text {
    Text::new(FontdueTextShaper::ubuntu_light().layout(text, &TextStyle::new(size, color), None))
}

fn layout_child(
    context: &mut LayoutContext<'_>,
    index: usize,
    width: f32,
    height: f32,
) -> torn::WidgetId {
    context
        .layout_child(index, tight(width, height))
        .expect("showcase child index is valid")
        .0
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
