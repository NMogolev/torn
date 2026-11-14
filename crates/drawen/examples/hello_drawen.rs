//! A native, interactive Drawen application for Windows and Linux.
//!
//! Run it with `cargo run -p drawen --example hello_drawen`. On Linux, an X11 or
//! Wayland session must be available to winit.

use std::{cell::Cell, rc::Rc};

use drawen::{
    Box as DrawenBox, Button, Color, Constraints, Size, Subscription, Text, UiRuntime,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
};

fn main() -> Result<(), drawen_platform_winit::RunError> {
    drawen_platform_winit::run(HelloDrawen::new())
}

struct HelloDrawen {
    runtime: UiRuntime,
    size: Size,
    _subscriptions: Vec<Subscription>,
}

impl HelloDrawen {
    fn new() -> Self {
        let clicks = Rc::new(Cell::new(0));
        let label = Text::new(FontdueTextShaper::ubuntu_light().layout(
            "Click the button",
            &TextStyle::new(20.0, Color::BLACK),
            None,
        ));
        let mut button = Button::new();
        button.set_backgrounds(
            Color::rgba8(180, 220, 255, 255),
            Color::rgba8(120, 180, 230, 255),
        );
        let subscription = button.activated().subscribe({
            let clicks = Rc::clone(&clicks);
            move |()| {
                clicks.set(clicks.get() + 1);
                println!("Clicked: {}", clicks.get());
            }
        });
        let mut root = DrawenBox::new();
        root.set_background(Some(Color::WHITE));
        let size = Size::new(480.0, 280.0).expect("initial window size is valid");
        let mut runtime = UiRuntime::new(root);
        let button = runtime
            .append_child(runtime.root(), button)
            .expect("root exists");
        runtime.append_child(button, label).expect("button exists");
        runtime
            .layout(Constraints::tight(size).expect("initial constraints are valid"))
            .expect("example widgets do not panic during layout");

        Self {
            runtime,
            size,
            _subscriptions: vec![subscription],
        }
    }

    fn layout(&mut self, size: Size) {
        self.size = size;
        if let Ok(constraints) = Constraints::tight(size) {
            let _ = self.runtime.layout(constraints);
        }
    }
}

impl WindowApplication for HelloDrawen {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Hello, Drawen", self.size)
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

    fn next_frame_in(&self) -> Option<std::time::Duration> {
        self.runtime.next_frame_in()
    }
}
