//! The smallest native Drawen window: a button with text and one event loop.
//!
//! Run it with `cargo run -p drawen --example minimal_window`. The window
//! adapter is a separate crate because headless applications do not need a
//! platform display server.

use drawen::{
    Box as DrawenBox, Button, Color, Constraints, Size, Subscription, Text, UiRuntime,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, PaintContext, TextStyle},
};

fn main() -> Result<(), drawen_platform_winit::RunError> {
    drawen_platform_winit::run(MinimalWindow::new())
}

struct MinimalWindow {
    runtime: UiRuntime,
    size: Size,
    _clicks: Subscription,
}

impl MinimalWindow {
    fn new() -> Self {
        let mut root = DrawenBox::new();
        root.set_background(Some(Color::WHITE));
        let size = Size::new(480.0, 280.0).expect("window size is valid");
        let mut runtime = UiRuntime::new(root);

        let button = Button::new();
        let clicks = button.activated().subscribe(|()| println!("Clicked"));
        let button = runtime
            .append_child(runtime.root(), button)
            .expect("root exists");
        runtime
            .append_child(
                button,
                Text::styled("Click me", TextStyle::new(16.0, Color::BLACK)),
            )
            .expect("button exists");
        runtime
            .layout(Constraints::tight(size).expect("window constraints are valid"))
            .expect("layout succeeds");

        Self {
            runtime,
            size,
            _clicks: clicks,
        }
    }

    fn layout(&mut self, size: Size) {
        self.size = size;
        if let Ok(constraints) = Constraints::tight(size) {
            let _ = self.runtime.layout(constraints);
        }
    }
}

impl WindowApplication for MinimalWindow {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Drawen", self.size)
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
            _ => WindowAction::None,
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
