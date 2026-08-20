//! An interactive showcase for Torn's current styling primitives.
//!
//! Run with `cargo run -p torn --example style_showcase`.
//!
//! The example deliberately uses only the public API available today: styled
//! [`torn::Box`] surfaces, stateful [`torn::Button`]s, runtime-owned children,
//! and a small custom layout widget. It does not pretend that gradients or
//! shadows exist before the renderer supports them.

use torn::{
    Border, Box as TornBox, Button, ChildLayout, Color, Constraints, Insets, LayoutContext,
    LayoutResult, Point, Rect, Size, Text, UiEnvironment, UiRuntime, Widget,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
};

const WINDOW_WIDTH: f32 = 1_120.0;
const WINDOW_HEIGHT: f32 = 720.0;
const PAGE_INSET: f32 = 48.0;
const HEADER_HEIGHT: f32 = 76.0;
const SECTION_GAP: f32 = 18.0;
const CARD_GAP: f32 = 16.0;

const CANVAS: Color = Color::rgba(10.0 / 255.0, 15.0 / 255.0, 29.0 / 255.0, 1.0);
const SURFACE: Color = Color::rgba(24.0 / 255.0, 33.0 / 255.0, 59.0 / 255.0, 1.0);
const SURFACE_BORDER: Color = Color::rgba(57.0 / 255.0, 76.0 / 255.0, 119.0 / 255.0, 1.0);
const CARD: Color = Color::rgba(18.0 / 255.0, 26.0 / 255.0, 47.0 / 255.0, 1.0);
const CARD_BORDER: Color = Color::rgba(43.0 / 255.0, 57.0 / 255.0, 90.0 / 255.0, 1.0);
const TEAL: Color = Color::rgba(46.0 / 255.0, 220.0 / 255.0, 179.0 / 255.0, 1.0);
const TEAL_HOVER: Color = Color::rgba(76.0 / 255.0, 235.0 / 255.0, 196.0 / 255.0, 1.0);
const TEAL_PRESSED: Color = Color::rgba(26.0 / 255.0, 188.0 / 255.0, 151.0 / 255.0, 1.0);
const LILAC: Color = Color::rgba(166.0 / 255.0, 135.0 / 255.0, 1.0, 1.0);
const TEXT: Color = Color::rgba(242.0 / 255.0, 245.0 / 255.0, 1.0, 1.0);
const MUTED: Color = Color::rgba(164.0 / 255.0, 178.0 / 255.0, 207.0 / 255.0, 1.0);

fn main() -> Result<(), torn_platform_winit::RunError> {
    torn_platform_winit::run(StyleShowcase::new())
}

struct StyleShowcase {
    runtime: UiRuntime,
    size: Size,
}

impl StyleShowcase {
    fn new() -> Self {
        let size = valid_size(WINDOW_WIDTH, WINDOW_HEIGHT);
        let mut runtime = UiRuntime::new(ShowcaseShell);
        let root = runtime.root();

        let library = Button::new()
            .with_background(Color::rgba8(36, 48, 79, 255))
            .with_hover_background(Color::rgba8(51, 68, 108, 255))
            .with_pressed_background(Color::rgba8(28, 38, 64, 255))
            .with_corner_radius(10.0)
            .with_border(Border::new(1.0, CARD_BORDER))
            .with_padding(Insets::all(8.0));
        library
            .activated()
            .subscribe(|()| println!("Открыта библиотека компонентов Torn."));
        let library = runtime
            .append_child(root, library)
            .expect("showcase root exists");
        runtime
            .append_child(library, label("Библиотека", 13.0, TEXT))
            .expect("library button exists");

        let hero = TornBox::new()
            .with_background(SURFACE)
            .with_corner_radius(24.0)
            .with_border(Border::new(1.0, SURFACE_BORDER))
            .with_padding(Insets::all(32.0));
        let hero = runtime
            .append_child(root, hero)
            .expect("showcase root exists");
        let hero_content = runtime
            .append_child(hero, HeroContent)
            .expect("hero surface exists");

        append_hero_button(
            &mut runtime,
            hero_content,
            &HeroAction::new(
                "Создать интерфейс",
                TEAL,
                TEAL_HOVER,
                TEAL_PRESSED,
                Color::rgba8(5, 29, 25, 255),
                "Основное действие активировано.",
            ),
        );
        append_hero_button(
            &mut runtime,
            hero_content,
            &HeroAction::new(
                "Открыть пример",
                Color::rgba8(42, 55, 91, 255),
                Color::rgba8(59, 77, 126, 255),
                Color::rgba8(31, 42, 71, 255),
                TEXT,
                "Пример открыт — его исходник уже у вас перед глазами.",
            ),
        );

        append_card(
            &mut runtime,
            root,
            CardContent::new(
                "01",
                "Поверхности",
                "Фон, радиус, рамка и внутренний отступ — локально на каждом контейнере.",
                TEAL,
            ),
        );
        append_card(
            &mut runtime,
            root,
            CardContent::new(
                "02",
                "Состояния",
                "Кнопка сама различает normal, hover и pressed, сохраняя свою логику отдельно.",
                LILAC,
            ),
        );
        append_card(
            &mut runtime,
            root,
            CardContent::new(
                "03",
                "Тема + стиль",
                "Тема задаёт разумный дефолт. Локальный стиль нужен только там, где он важен.",
                Color::rgba8(255, 184, 92, 255),
            ),
        );

        let mut app = Self { runtime, size };
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
        WindowOptions::new("Torn — Style Showcase", self.size)
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

/// Places the showcase header, hero card, and feature cards responsively.
struct ShowcaseShell;

impl Widget for ShowcaseShell {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let inset = PAGE_INSET
            .min(size.width() * 0.08)
            .min(size.height() * 0.08);
        let content_width = (size.width() - inset * 2.0).max(0.0);
        let header_height = HEADER_HEIGHT.min((size.height() - inset * 2.0).max(0.0));
        let hero_y = inset + header_height;
        let usable_height = (size.height() - hero_y - inset).max(0.0);
        let hero_height = usable_height.clamp(180.0, 326.0).min(usable_height);
        let cards_y =
            hero_y + hero_height + SECTION_GAP.min((usable_height - hero_height).max(0.0));
        let cards_height = (size.height() - cards_y - inset).max(0.0);

        let (library, _) = context
            .layout_child(
                0,
                tight(112.0_f32.min(content_width), 36.0_f32.min(header_height)),
            )
            .expect("library button is the first shell child");
        let (hero, _) = context
            .layout_child(1, tight(content_width, hero_height))
            .expect("hero is the second shell child");

        let card_count = context.child_count().saturating_sub(2).min(3);
        let (gap_count, card_divisor) = match card_count {
            0 | 1 => (0.0, 1.0),
            2 => (1.0, 2.0),
            _ => (2.0, 3.0),
        };
        let card_width = ((content_width - CARD_GAP * gap_count) / card_divisor).max(0.0);
        let mut children = vec![
            ChildLayout::new(
                library,
                Point::new(inset + (content_width - 112.0).max(0.0), inset),
            ),
            ChildLayout::new(hero, Point::new(inset, hero_y)),
        ];
        for index in 0..card_count {
            let (card, _) = context
                .layout_child(index + 2, tight(card_width, cards_height))
                .expect("feature card follows hero");
            children.push(ChildLayout::new(
                card,
                Point::new(
                    inset
                        + (card_width + CARD_GAP)
                            * match index {
                                0 => 0.0,
                                1 => 1.0,
                                _ => 2.0,
                            },
                    cards_y,
                ),
            ));
        }
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, CANVAS);
        let shaper = FontdueTextShaper::ubuntu_light();
        context.draw_text(
            shaper.layout("TORN / COMPONENT LAB", &TextStyle::new(12.0, TEAL), None),
            Point::new(bounds.origin.x + PAGE_INSET, bounds.origin.y + 24.0),
        );
        context.draw_text(
            shaper.layout(
                "Интерфейс — это ваш язык.",
                &TextStyle::new(26.0, TEXT),
                None,
            ),
            Point::new(bounds.origin.x + PAGE_INSET, bounds.origin.y + 43.0),
        );
    }
}

/// Paints the hero copy and positions its two runtime-owned action buttons.
struct HeroContent;

impl Widget for HeroContent {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let button_y = (size.height() - 44.0).max(112.0);
        let first_width = 166.0_f32.min(size.width());
        let second_width = 144.0_f32.min((size.width() - first_width - 12.0).max(0.0));
        let mut children = Vec::new();
        if context.child_count() > 0 {
            let (primary, _) = context
                .layout_child(0, tight(first_width, 44.0_f32.min(size.height())))
                .expect("primary action is the first hero child");
            children.push(ChildLayout::new(primary, Point::new(0.0, button_y)));
        }
        if context.child_count() > 1 {
            let (secondary, _) = context
                .layout_child(1, tight(second_width, 44.0_f32.min(size.height())))
                .expect("secondary action is the second hero child");
            children.push(ChildLayout::new(
                secondary,
                Point::new(first_width + 12.0, button_y),
            ));
        }
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let shaper = FontdueTextShaper::ubuntu_light();
        context.draw_text(
            shaper.layout(
                "Стиль — не клетка, а набор возможностей.",
                &TextStyle::new(30.0, TEXT),
                None,
            ),
            Point::new(bounds.origin.x, bounds.origin.y + 2.0),
        );
        context.draw_text(
            shaper.layout(
                "Собирайте строгие ретро-панели, мягкие карточки или собственный визуальный язык.\nТема даёт старт, а каждый виджет можно настроить точечно.",
                &TextStyle::new(15.0, MUTED),
                None,
            ),
            Point::new(bounds.origin.x, bounds.origin.y + 52.0),
        );

        let preview = Rect::new(
            Point::new(
                bounds.origin.x + (bounds.size.width() - 206.0).max(0.0),
                bounds.origin.y + 12.0,
            ),
            valid_size(
                194.0_f32.min(bounds.size.width()),
                122.0_f32.min(bounds.size.height()),
            ),
        );
        context.fill_rounded_rect(preview, 18.0, Color::rgba8(15, 22, 42, 255));
        context.stroke_rounded_rect(preview, 18.0, 1.0, Color::rgba8(79, 100, 148, 255));
        for (offset, color) in [
            (18.0, TEAL),
            (52.0, LILAC),
            (86.0, Color::rgba8(255, 184, 92, 255)),
        ] {
            context.fill_rounded_rect(
                Rect::new(
                    Point::new(preview.origin.x + 16.0, preview.origin.y + offset),
                    valid_size(162.0, 14.0),
                ),
                7.0,
                color,
            );
        }
    }
}

struct CardContent {
    number: &'static str,
    title: &'static str,
    body: &'static str,
    accent: Color,
}

impl CardContent {
    const fn new(
        number: &'static str,
        title: &'static str,
        body: &'static str,
        accent: Color,
    ) -> Self {
        Self {
            number,
            title,
            body,
            accent,
        }
    }
}

impl Widget for CardContent {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(available_size(constraints))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        let shaper = FontdueTextShaper::ubuntu_light();
        context.fill_rounded_rect(
            Rect::new(bounds.origin, valid_size(42.0, 26.0)),
            13.0,
            self.accent,
        );
        context.draw_text(
            shaper.layout(self.number, &TextStyle::new(12.0, CANVAS), None),
            Point::new(bounds.origin.x + 12.0, bounds.origin.y + 6.0),
        );
        context.draw_text(
            shaper.layout(self.title, &TextStyle::new(19.0, TEXT), None),
            Point::new(bounds.origin.x, bounds.origin.y + 48.0),
        );
        context.draw_text(
            shaper.layout(self.body, &TextStyle::new(14.0, MUTED), None),
            Point::new(bounds.origin.x, bounds.origin.y + 82.0),
        );
        let line_y = bounds.origin.y + (bounds.size.height() - 12.0).max(100.0);
        context.fill_rounded_rect(
            Rect::new(
                Point::new(bounds.origin.x, line_y),
                valid_size(bounds.size.width().min(96.0), 4.0),
            ),
            2.0,
            self.accent,
        );
    }
}

struct HeroAction {
    text: &'static str,
    background: Color,
    hover: Color,
    pressed: Color,
    foreground: Color,
    message: &'static str,
}

impl HeroAction {
    const fn new(
        text: &'static str,
        background: Color,
        hover: Color,
        pressed: Color,
        foreground: Color,
        message: &'static str,
    ) -> Self {
        Self {
            text,
            background,
            hover,
            pressed,
            foreground,
            message,
        }
    }
}

fn append_hero_button(runtime: &mut UiRuntime, parent: torn::WidgetId, action: &HeroAction) {
    let button = Button::new()
        .with_background(action.background)
        .with_hover_background(action.hover)
        .with_pressed_background(action.pressed)
        .with_corner_radius(12.0)
        .with_border(Border::new(1.0, Color::rgba8(92, 112, 158, 255)))
        .with_padding(Insets::all(8.0));
    let message = action.message;
    button
        .activated()
        .subscribe(move |()| println!("{message}"));
    let button = runtime
        .append_child(parent, button)
        .expect("hero content exists");
    runtime
        .append_child(button, label(action.text, 13.0, action.foreground))
        .expect("hero action exists");
}

fn append_card(runtime: &mut UiRuntime, parent: torn::WidgetId, content: CardContent) {
    let card = TornBox::new()
        .with_background(CARD)
        .with_corner_radius(18.0)
        .with_border(Border::new(1.0, CARD_BORDER))
        .with_padding(Insets::all(22.0));
    let card = runtime
        .append_child(parent, card)
        .expect("showcase root exists");
    runtime
        .append_child(card, content)
        .expect("feature card exists");
}

fn label(text: &str, size: f32, color: Color) -> Text {
    Text::new(FontdueTextShaper::ubuntu_light().layout(text, &TextStyle::new(size, color), None))
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
