//! The canonical Drawen application-state example.
//!
//! State lives in the application, widgets emit typed signals, and the event
//! loop turns a state change into one layout or paint request. Widgets never
//! own timers, threads, or a second event loop.
//!
//! Run it with `cargo run -p drawen --example stateful_app`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use drawen::{
    Button, ChildLayout, Color, Constraints, LayoutContext, LayoutResult, Point, Rect, Size,
    Subscription, Text, UiEnvironment, UiRuntime, Widget,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
};

fn main() -> Result<(), drawen_platform_winit::RunError> {
    drawen_platform_winit::run(CounterApp::new())
}

#[derive(Default)]
struct AppState {
    count: u32,
}

/// A presentation widget that reads shared application state while painting.
///
/// It has no signal of its own: the button subscription marks the application
/// dirty, and the event loop invalidates paint before asking for a new frame.
struct CounterView {
    state: Rc<RefCell<AppState>>,
}

impl Widget for CounterView {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let (button, _) = context
            .layout_child(0, Constraints::loose(size))
            .expect("counter view owns one button");
        LayoutResult::with_children(
            size,
            vec![ChildLayout::new(button, Point::new(24.0, 120.0))],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, Color::WHITE);

        let shaper = FontdueTextShaper::ubuntu_light();
        let title = shaper.layout(
            "Drawen counter",
            &TextStyle::new(14.0, Color::rgba8(90, 100, 110, 255)),
            None,
        );
        context.draw_text(
            title,
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 24.0),
        );

        let count = self.state.borrow().count.to_string();
        let count = shaper.layout(&count, &TextStyle::new(48.0, Color::BLACK), None);
        context.draw_text(
            count,
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 44.0),
        );
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

struct CounterApp {
    runtime: UiRuntime,
    size: Size,
    dirty: Rc<Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

impl CounterApp {
    fn new() -> Self {
        let state = Rc::new(RefCell::new(AppState::default()));
        let dirty = Rc::new(Cell::new(false));
        let mut runtime = UiRuntime::new(CounterView {
            state: Rc::clone(&state),
        });

        let button = Button::new();
        let subscription = button.activated().subscribe({
            let state = Rc::clone(&state);
            let dirty = Rc::clone(&dirty);
            move |()| {
                state.borrow_mut().count += 1;
                dirty.set(true);
            }
        });
        let button = runtime
            .append_child(runtime.root(), button)
            .expect("root exists");
        runtime
            .append_child(
                button,
                Text::styled("Increment", TextStyle::new(16.0, Color::BLACK)),
            )
            .expect("button exists");

        let size = Size::new(480.0, 280.0).expect("window size is valid");
        runtime
            .layout(Constraints::tight(size).expect("window constraints are valid"))
            .expect("layout succeeds");

        Self {
            runtime,
            size,
            dirty,
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

impl WindowApplication for CounterApp {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Drawen state", self.size)
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
                let state_changed = self.dirty.replace(false);
                let widget_changed = self.runtime.take_redraw_request();
                if state_changed {
                    self.runtime.invalidate_paint();
                }
                if state_changed || widget_changed {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
            }
            // These events change whether a frame is worth drawing, not the
            // application state. The platform asks for a redraw on restore.
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

fn available_size(constraints: Constraints) -> Size {
    let maximum = constraints.max();
    let minimum = constraints.min();
    constraints.constrain(
        Size::new(
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
        )
        .expect("layout dimensions are finite and non-negative"),
    )
}
