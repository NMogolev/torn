//! Off-thread work with a `TaskPool` and a UI dispatcher.
//!
//! The button starts a job on a worker thread. The job reports progress and a
//! final value through a `UiDispatcher`; the UI thread drains those messages in
//! `WindowEvent::RedrawRequested` and repaints. Widgets, the runtime, and
//! signals never leave the UI thread.
//!
//! Run it with `cargo run -p drawen --example background_tasks`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use drawen::{
    Button, ChildLayout, Color, Constraints, LayoutContext, LayoutResult, Point, Rect, Size,
    Subscription, Text, UiEnvironment, UiRuntime, Widget,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions, WindowWaker},
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
    tasks::{TaskPool, UiDispatcher, UiSender},
};

const STEPS: u64 = 100;

fn main() -> Result<(), drawen_platform_winit::RunError> {
    drawen_platform_winit::run(TaskApp::new())
}

enum Message {
    Progress(f32),
    Finished(u64),
}

struct TaskState {
    pool: TaskPool,
    sender: RefCell<Option<UiSender<Message>>>,
    progress: Cell<f32>,
    status: RefCell<String>,
    busy: Cell<bool>,
}

impl TaskState {
    fn new() -> Self {
        Self {
            pool: TaskPool::new(2).expect("worker threads start"),
            sender: RefCell::new(None),
            progress: Cell::new(0.0),
            status: RefCell::new(String::from("Idle")),
            busy: Cell::new(false),
        }
    }

    fn start(&self) {
        if self.busy.replace(true) {
            return;
        }
        self.progress.set(0.0);
        *self.status.borrow_mut() = String::from("Running");
        let Some(sender) = self.sender.borrow().clone() else {
            self.busy.set(false);
            return;
        };

        let progress_sender = sender.clone();
        let finish_sender = sender;
        let handle = self.pool.spawn_with_progress(
            move |context| {
                let mut checksum = 0u64;
                for step in 0..STEPS {
                    if context.is_cancelled() {
                        return;
                    }
                    // Stand-in for decoding, parsing, or network work.
                    std::thread::sleep(Duration::from_millis(8));
                    checksum = checksum.wrapping_add(step * step);
                    context.report_units(step + 1, STEPS);
                }
                let _ = finish_sender.send(Message::Finished(checksum));
            },
            move |progress| {
                let _ = progress_sender.send(Message::Progress(progress.fraction().unwrap_or(0.0)));
            },
        );
        // The UI does not join; the dispatcher delivers the result.
        drop(handle);
    }
}

struct TaskView {
    state: Rc<TaskState>,
}

impl Widget for TaskView {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let (button, _) = context
            .layout_child(0, Constraints::loose(size))
            .expect("task view owns one button");
        LayoutResult::with_children(
            size,
            vec![ChildLayout::new(button, Point::new(24.0, 150.0))],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, Color::WHITE);
        let shaper = FontdueTextShaper::ubuntu_light();

        let title = shaper.layout(
            "Background work",
            &TextStyle::new(14.0, Color::rgba8(90, 100, 110, 255)),
            None,
        );
        context.draw_text(
            title,
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 24.0),
        );

        let status = self.state.status.borrow().clone();
        let status = shaper.layout(&status, &TextStyle::new(28.0, Color::BLACK), None);
        context.draw_text(
            status,
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 52.0),
        );

        let track = Rect::new(
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 100.0),
            Size::new(bounds.size.width() - 48.0, 10.0)
                .expect("the view is wider than its margins"),
        );
        context.fill_rect(track, Color::rgba8(228, 232, 238, 255));
        let progress = self.state.progress.get().clamp(0.0, 1.0);
        if progress > 0.0 {
            let filled = Rect::new(
                track.origin,
                Size::new(track.size.width() * progress, track.size.height())
                    .expect("progress is finite and non-negative"),
            );
            context.fill_rect(filled, Color::rgba8(52, 120, 210, 255));
        }

        let hint = shaper.layout(
            "The UI thread never joins a worker.",
            &TextStyle::new(13.0, Color::rgba8(120, 128, 138, 255)),
            None,
        );
        context.draw_text(
            hint,
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 124.0),
        );
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

struct TaskApp {
    runtime: UiRuntime,
    size: Size,
    state: Rc<TaskState>,
    dispatcher: Option<UiDispatcher<Message>>,
    _subscriptions: Vec<Subscription>,
}

impl TaskApp {
    fn new() -> Self {
        let state = Rc::new(TaskState::new());
        let mut runtime = UiRuntime::new(TaskView {
            state: Rc::clone(&state),
        });

        let button = Button::new();
        let subscription = button.activated().subscribe({
            let state = Rc::clone(&state);
            move |()| state.start()
        });
        let button = runtime
            .append_child(runtime.root(), button)
            .expect("root exists");
        runtime
            .append_child(
                button,
                Text::styled("Run 100 steps", TextStyle::new(16.0, Color::BLACK)),
            )
            .expect("button exists");

        let size = Size::new(480.0, 240.0).expect("window size is valid");
        runtime
            .layout(Constraints::tight(size).expect("window constraints are valid"))
            .expect("layout succeeds");

        Self {
            runtime,
            size,
            state,
            dispatcher: None,
            _subscriptions: vec![subscription],
        }
    }

    fn layout(&mut self, size: Size) {
        self.size = size;
        if let Ok(constraints) = Constraints::tight(size) {
            let _ = self.runtime.layout(constraints);
        }
    }

    fn drain_messages(&mut self) -> bool {
        let Some(dispatcher) = &self.dispatcher else {
            return false;
        };
        let messages = dispatcher.drain();
        if messages.is_empty() {
            return false;
        }
        for message in messages {
            match message {
                Message::Progress(fraction) => self.state.progress.set(fraction),
                Message::Finished(checksum) => {
                    self.state.busy.set(false);
                    *self.state.status.borrow_mut() = format!("Checksum {checksum}");
                }
            }
        }
        true
    }
}

impl WindowApplication for TaskApp {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Drawen background tasks", self.size)
    }

    fn attach_waker(&mut self, waker: WindowWaker) {
        let (dispatcher, sender) = UiDispatcher::new(move || waker.wake());
        *self.state.sender.borrow_mut() = Some(sender);
        self.dispatcher = Some(dispatcher);
    }

    fn window_event(&mut self, event: WindowEvent) -> WindowAction {
        match event {
            WindowEvent::CloseRequested => WindowAction::Exit,
            WindowEvent::Resized(size) => {
                self.layout(size);
                WindowAction::RequestRedraw
            }
            WindowEvent::RedrawRequested => {
                if self.drain_messages() {
                    self.runtime.invalidate_paint();
                }
                WindowAction::None
            }
            WindowEvent::Input(input) => {
                let _ = self.runtime.dispatch_event(&input);
                if self.runtime.take_redraw_request() {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
            }
            WindowEvent::Visible(_) | WindowEvent::Minimized(_) | WindowEvent::Occluded(_) => {
                WindowAction::None
            }
            WindowEvent::Maximized(_) | WindowEvent::Fullscreen(_) => WindowAction::RequestRedraw,
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
