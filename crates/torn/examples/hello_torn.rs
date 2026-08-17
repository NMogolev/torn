//! A native, interactive Torn application for Windows and Linux.
//!
//! Run it with `cargo run -p torn --example hello_torn`. On Linux, an X11 or
//! Wayland session must be available to winit.

use std::{cell::Cell, rc::Rc};

use torn::{
    Box as TornBox, Button, Color, Constraints, Size, Text, UiRuntime,
    platform::{Frame, WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, PaintContext, TextLayout},
    software::{PixelBuffer, SoftwareRenderer},
};

fn main() -> Result<(), torn_platform_winit::RunError> {
    torn_platform_winit::run(HelloTorn::new())
}

struct HelloTorn {
    runtime: UiRuntime,
    size: Size,
    clicks: Rc<Cell<u32>>,
}

impl HelloTorn {
    fn new() -> Self {
        let clicks = Rc::new(Cell::new(0));
        let label = Text::new(TextLayout::new(
            Size::new(144.0, 20.0).expect("fixed label size is valid"),
            Color::BLACK,
        ));
        let mut button = Button::new(label);
        button.set_backgrounds(
            Color::rgba8(180, 220, 255, 255),
            Color::rgba8(120, 180, 230, 255),
        );
        button.set_on_click({
            let clicks = Rc::clone(&clicks);
            move || {
                clicks.set(clicks.get() + 1);
                println!("Нажато: {}", clicks.get());
            }
        });
        let mut root = TornBox::with_child(button);
        root.set_background(Some(Color::WHITE));
        let size = Size::new(480.0, 280.0).expect("initial window size is valid");
        let mut runtime = UiRuntime::new(root);
        runtime
            .layout(Constraints::tight(size).expect("initial constraints are valid"))
            .expect("example widgets do not panic during layout");

        Self {
            runtime,
            size,
            clicks,
        }
    }

    fn layout(&mut self, size: Size) {
        self.size = size;
        if let Ok(constraints) = Constraints::tight(size) {
            let _ = self.runtime.layout(constraints);
        }
    }
}

impl WindowApplication for HelloTorn {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Hello, Torn", self.size)
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

    fn redraw(&mut self, frame: &mut Frame<'_>) {
        let width = pixel_extent(self.size.width());
        let height = pixel_extent(self.size.height());
        let mut display_list = DisplayList::new();
        if self
            .runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .is_err()
        {
            return;
        }
        let Ok(mut image) = PixelBuffer::new(width, height) else {
            return;
        };
        if SoftwareRenderer.render(&display_list, &mut image).is_err() {
            return;
        }
        for (destination, source) in frame.pixels_mut().chunks_exact_mut(4).zip(image.pixels()) {
            destination.copy_from_slice(&[source.red, source.green, source.blue, source.alpha]);
        }
        let _ = self.clicks.get();
    }
}

fn pixel_extent(value: f32) -> u32 {
    let value = value.round().max(1.0);
    if value >= 4_294_967_000.0 {
        return u32::MAX;
    }
    // The lower bound and explicit upper guard establish a valid u32 range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        value as u32
    }
}
