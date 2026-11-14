//! A complete headless Drawen pipeline using only the public `drawen` facade.

use std::{cell::Cell, path::PathBuf, rc::Rc};

use drawen::{
    Box as DrawenBox, Button, Color, Constraints, InputEvent, Modifiers, Point, PointerButton,
    PointerButtons, PointerEvent, PointerId, Size, Text, UiRuntime,
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
    software::{PixelBuffer, SoftwareRenderer},
};

fn main() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    let click_count = Rc::new(Cell::new(0));
    let label = Text::new(FontdueTextShaper::ubuntu_light().layout(
        "Click the button",
        &TextStyle::new(16.0, Color::BLACK),
        None,
    ));
    let mut button = Button::new();
    button.set_backgrounds(
        Color::rgba8(180, 220, 255, 255),
        Color::rgba8(120, 180, 230, 255),
    );
    let _subscription = button.activated().subscribe({
        let click_count = Rc::clone(&click_count);
        move |()| click_count.set(click_count.get() + 1)
    });

    let mut root = DrawenBox::new();
    root.set_background(Some(Color::WHITE));
    let mut runtime = UiRuntime::new(root);
    let button = runtime.append_child(runtime.root(), button)?;
    runtime.append_child(button, label)?;
    let canvas = size(320.0, 180.0)?;

    runtime.layout(Constraints::tight(canvas)?)?;

    assert_eq!(
        runtime.dispatch_event(&pointer_down(Point::new(12.0, 12.0))),
        drawen::EventStatus::Handled
    );
    assert_eq!(click_count.get(), 0);
    assert_eq!(
        runtime.dispatch_event(&pointer_up(Point::new(12.0, 12.0))),
        drawen::EventStatus::Handled
    );
    assert_eq!(click_count.get(), 1);

    let mut display_list = DisplayList::new();
    runtime.paint(&mut PaintContext::new(&mut display_list))?;

    let mut pixels = PixelBuffer::new(320, 180)?;
    SoftwareRenderer.render(&display_list, &mut pixels)?;
    let output = PathBuf::from("target/drawen-tutorial.png");
    std::fs::create_dir_all("target")?;
    pixels.write_png(&output)?;

    println!("Handled the event {} time(s).", click_count.get());
    println!("Wrote the PNG to {}.", output.display());
    Ok(())
}

fn size(width: f32, height: f32) -> Result<Size, drawen::SizeError> {
    Size::new(width, height)
}

fn pointer_down(position: Point) -> InputEvent {
    InputEvent::PointerDown(pointer_event(position, PointerButtons::PRIMARY))
}

fn pointer_up(position: Point) -> InputEvent {
    InputEvent::PointerUp(pointer_event(position, PointerButtons::NONE))
}

fn pointer_event(position: Point, buttons: PointerButtons) -> PointerEvent {
    PointerEvent {
        pointer_id: PointerId(1),
        position,
        button: Some(PointerButton::Primary),
        buttons,
        modifiers: Modifiers::NONE,
    }
}
