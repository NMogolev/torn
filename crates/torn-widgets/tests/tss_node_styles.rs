//! Integration tests for per-node TSS styles on standard widgets.

use torn_core::{
    Color, Constraints, InputEvent, Modifiers, Point, PointerButtons, PointerEvent, PointerId, Size,
};
use torn_render::{DisplayCommand, DisplayList, FontdueTextShaper, PaintContext, TextStyle};
use torn_ui::{LightTheme, StyleTarget, Styled, UiEnvironment, UiRuntime};
use torn_widgets::{Button, Checkbox, Stack, Text};

fn environment(source: &str) -> UiEnvironment {
    UiEnvironment::from_tss(LightTheme, source).expect("valid TSS")
}

fn pointer_move(position: Point) -> InputEvent {
    InputEvent::PointerMove(PointerEvent {
        pointer_id: PointerId(1),
        position,
        button: None,
        buttons: PointerButtons::NONE,
        modifiers: Modifiers::NONE,
    })
}

fn pointer_down(position: Point) -> InputEvent {
    InputEvent::PointerDown(PointerEvent {
        pointer_id: PointerId(1),
        position,
        button: Some(torn_core::PointerButton::Primary),
        buttons: PointerButtons::PRIMARY,
        modifiers: Modifiers::NONE,
    })
}

#[test]
fn type_rules_style_only_matching_widgets() {
    let mut runtime =
        UiRuntime::with_environment(Stack::new(), environment("button { background: #112233; }"));
    let button = runtime
        .append_child(runtime.root(), Button::new())
        .expect("root exists");
    let checkbox = runtime
        .append_child(runtime.root(), Checkbox::new())
        .expect("root exists");
    runtime
        .layout(Constraints::UNBOUNDED)
        .expect("layout succeeds");

    assert_eq!(
        runtime
            .computed_style(button)
            .expect("button is live")
            .background,
        Some(Color::rgba8(17, 34, 51, 255))
    );
    assert_eq!(
        runtime
            .computed_style(checkbox)
            .expect("checkbox is live")
            .background,
        None
    );
}

#[test]
fn id_and_class_rules_apply_through_styled() {
    let mut runtime = UiRuntime::with_environment(
        Stack::new(),
        environment(
            "#save { background: #00ff00; } \
             .primary { border-radius: 7px; } \
             button.primary { color: #0000ff; }",
        ),
    );
    let plain = runtime
        .append_child(runtime.root(), Button::new())
        .expect("root exists");
    let styled = runtime
        .append_child(
            runtime.root(),
            Styled::new(Button::new())
                .with_id("save")
                .with_class("primary"),
        )
        .expect("root exists");
    runtime
        .layout(Constraints::UNBOUNDED)
        .expect("layout succeeds");

    let style = runtime
        .computed_style(styled)
        .expect("styled button is live");
    assert_eq!(style.background, Some(Color::rgba8(0, 255, 0, 255)));
    assert_eq!(style.corner_radius, Some(7.0));
    assert_eq!(style.foreground, Some(Color::rgba8(0, 0, 255, 255)));
    assert_eq!(
        runtime.computed_style(plain).expect("plain button is live"),
        torn_ui::TssStyle::default()
    );
    let expected_target = StyleTarget::new("button")
        .with_id("save")
        .with_class("primary");
    assert_eq!(runtime.style_target(styled), Some(&expected_target));
}

#[test]
fn hover_and_focus_states_participate_in_the_cascade() {
    let mut runtime = UiRuntime::with_environment(
        Button::new(),
        environment("button:hover { background: #101112; } button:focus { border-radius: 5px; }"),
    );
    let button = runtime.root();
    runtime
        .layout(
            Constraints::tight(Size::new(40.0, 40.0).expect("valid size"))
                .expect("valid constraints"),
        )
        .expect("layout succeeds");

    let _ = runtime.dispatch_event(&pointer_move(Point::new(20.0, 20.0)));
    let mut list = DisplayList::new();
    runtime
        .paint(&mut PaintContext::new(&mut list))
        .expect("hover paint succeeds");
    assert_eq!(
        runtime
            .computed_style(button)
            .expect("button is live")
            .background,
        Some(Color::rgba8(16, 17, 18, 255))
    );

    let _ = runtime.dispatch_event(&pointer_down(Point::new(20.0, 20.0)));
    let mut list = DisplayList::new();
    runtime
        .paint(&mut PaintContext::new(&mut list))
        .expect("focus paint succeeds");
    assert_eq!(runtime.focused_widget(), Some(button));
    assert_eq!(
        runtime
            .computed_style(button)
            .expect("button is live")
            .corner_radius,
        Some(5.0)
    );
}

#[test]
fn inherited_text_style_reaches_descendants() {
    let mut runtime = UiRuntime::with_environment(
        Button::new(),
        environment("button { color: #112233; font-size: 22px; }"),
    );
    let button = runtime.root();
    let label = FontdueTextShaper::ubuntu_light().layout(
        "Inherit",
        &TextStyle::new(10.0, Color::BLACK),
        None,
    );
    let text = runtime
        .append_child(button, Text::new(label))
        .expect("button exists");
    runtime
        .layout(Constraints::UNBOUNDED)
        .expect("layout succeeds");

    let style = runtime.computed_style(text).expect("text is live");
    assert_eq!(style.foreground, Some(Color::rgba8(17, 34, 51, 255)));
    assert_eq!(style.font_size, Some(22.0));

    let mut list = DisplayList::new();
    runtime
        .paint(&mut PaintContext::new(&mut list))
        .expect("paint succeeds");
    let drawn = list
        .commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { layout, .. } => Some(layout),
            _ => None,
        })
        .expect("text is painted");
    assert_eq!(drawn.color(), Color::rgba8(17, 34, 51, 255));
}

#[test]
fn rgba_borders_parse_and_apply_to_standard_widgets() {
    let mut runtime = UiRuntime::with_environment(
        Button::new(),
        environment("button { border: 2px solid rgba(12, 34, 56, 0.5); }"),
    );
    let button = runtime.root();
    runtime
        .layout(
            Constraints::tight(Size::new(30.0, 30.0).expect("valid size"))
                .expect("valid constraints"),
        )
        .expect("layout succeeds");

    let border = runtime
        .computed_style(button)
        .expect("button is live")
        .border
        .expect("border is resolved");
    assert!((border.width - 2.0).abs() < f32::EPSILON);
    assert_eq!(border.color, Color::rgba8(12, 34, 56, 128));

    let mut list = DisplayList::new();
    runtime
        .paint(&mut PaintContext::new(&mut list))
        .expect("paint succeeds");
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::StrokeRoundedRect {
            width: 2.0,
            color,
            ..
        } if *color == border.color
    )));
}
