# Background work and threading

Drawen has exactly one UI thread per window. The widget tree, `UiRuntime`,
`Signal`, `Subscription`, theme, and language services are UI-thread-local and
are neither `Send` nor `Sync`. Widgets never spawn threads, own timers, or call
platform APIs.

`drawen-tasks` is the sanctioned boundary around that rule:

```text
worker thread                     UI thread
-------------                     ---------
TaskPool::spawn(...)  ──owned──▶  UiDispatcher::drain()
owned data only                    widgets, signals, state, paint
```

Workers own their inputs and return owned values. Only plain data crosses the
thread boundary; the UI thread applies it to widgets and asks for one redraw.

## Quick start

```rust
use drawen::{
    platform::{WindowApplication, WindowEvent, WindowWaker},
    tasks::{TaskPool, UiDispatcher},
};

enum Message {
    Progress { done: u64, total: u64 },
    Finished(u64),
}

struct App {
    pool: TaskPool,
    dispatcher: Option<UiDispatcher<Message>>,
    sender: Option<drawen::tasks::UiSender<Message>>,
    result: u64,
}

impl App {
    fn new() -> Self {
        Self {
            pool: TaskPool::new(2).expect("worker threads start"),
            dispatcher: None,
            sender: None,
            result: 0,
        }
    }
}

impl WindowApplication for App {
    // ...

    fn attach_waker(&mut self, waker: WindowWaker) {
        let (dispatcher, sender) = UiDispatcher::new(move || waker.wake());
        self.dispatcher = Some(dispatcher);
        self.sender = Some(sender);
    }

    fn window_event(&mut self, event: WindowEvent) -> drawen::platform::WindowAction {
        match event {
            WindowEvent::RedrawRequested => {
                if let Some(dispatcher) = &self.dispatcher {
                    for message in dispatcher.drain() {
                        match message {
                            Message::Progress { .. } => {}
                            Message::Finished(value) => self.result = value,
                        }
                    }
                }
                drawen::platform::WindowAction::None
            }
            // ...
            _ => drawen::platform::WindowAction::None,
        }
    }

    // ...
}

fn start(app: &mut App) {
    let Some(sender) = app.sender.clone() else {
        return;
    };
    let progress = sender.clone();
    app.pool.spawn_with_progress(
        move |context| {
            for step in 0..100 {
                if context.is_cancelled() {
                    return;
                }
                context.report_units(step + 1, 100);
                // decode, parse, hash, copy, or fetch here
            }
            let _ = sender.send(Message::Finished(42));
        },
        move |snapshot| {
            let done = snapshot.completed();
            let total = snapshot.total().unwrap_or(0);
            let _ = progress.send(Message::Progress { done, total });
        },
    );
}
```

The native `winit` adapter calls `attach_waker` before the first event. The
waker sends a user event; the adapter turns it into a redraw request, so
`WindowEvent::RedrawRequested` is the single place where queued values become
widget state.

## Keeping the UI responsive

- Never call `TaskHandle::join` on the UI thread. Store the handle, poll
  `is_finished`, or let the task deliver its own result through the dispatcher.
- Drain the dispatcher once per event-loop turn, apply all values, then
  `invalidate_paint()` and return `WindowAction::RequestRedraw` only if the
  state changed.
- The dispatcher channel is unbounded. If a producer can outrun the UI, send
  the newest value only (replace state) instead of a complete history.
- `UiDispatcher::new` calls the wake callback after every successful send. The
  callback should only wake the event loop, never touch widgets.
- `UiSender::send` returns `UiSendError` with the value back when the window
  has closed, so a worker can stop early instead of computing for nothing.

## Cancellation

Cancellation is cooperative and never interrupts a running function:

```rust
let handle = pool.spawn_with_context(|context| {
    for item in items {
        if context.check_cancelled().is_err() {
            return None;
        }
        process(item);
    }
    Some(result)
});

// From the UI thread, for example on a Cancel button:
handle.cancel();
```

A cancelled task still runs to its next checkpoint and still returns a value;
the handle observes that value through `join` or the dispatcher. Cancelling a
queued task does not remove it from the queue.

## Progress

`TaskContext::report_units(completed, total)` and `TaskContext::report` take a
snapshot. `TaskProgress::fraction()` returns a clamped `Option<f32>` for
determinate progress; `None` means the total is unknown. The callback runs on
the worker thread, so forward the snapshot through the dispatcher and paint a
`ProgressBar` from UI-thread state.

## Pool lifecycle

- A pool has a fixed worker count. `TaskPool::new(0)` clamps to one;
  `TaskPool::for_available_parallelism()` uses one thread per hardware thread.
- Dropping the pool, or calling `shutdown`, stops new work, finishes jobs that
  are already queued, and joins every worker. Dropping the pool from one of its
  own tasks deadlocks.
- A panic inside a task is caught and returned by `TaskHandle::join` as
  `TaskJoinError::Panicked`. The worker thread survives.
- `queued_tasks`, `active_tasks`, `worker_count`, and `is_idle` report pool
  state for diagnostics.

Use one pool per application, not one per window. A single pool keeps the
number of hardware threads under control and shuts down deterministically.

## What must not cross a thread

| UI-thread-only | Worker-safe |
| --- | --- |
| `UiRuntime`, any `Widget` | owned data, `Arc<Mutex<_>>` |
| `Signal<T>`, `Subscription` | `UiSender<T>` |
| `UiEnvironment`, `Theme` | `TaskHandle<T>` · results via dispatcher |
| `Rc<RefCell<_>>` model state | `Arc` + synchronization |

The `Signal` API is deliberately not thread-safe. To notify the UI from a
worker, send a message through a `UiDispatcher`; subscribe to a signal only on
the UI thread.

## Troubleshooting

- **The window never updates after a task finishes.** The dispatcher's wake
  callback is missing or does nothing. Use `WindowWaker` from
  `WindowApplication::attach_waker`, or return a frame interval from
  `next_frame_in` while work is pending.
- **The UI freezes.** A `join` or a blocking channel `recv` is running on the UI
  thread. Replace it with `is_finished`, `drain`, or `try_recv`.
- **A task captures a widget and no longer compiles.** That is the thread-safety
  boundary working as intended. Move owned data into the task and rebuild or
  update the widget on the UI thread.
- **Progress jumps or is delayed.** The wake callback coalesces nothing; if
  the UI is busy, multiple snapshots queue up. Send fewer snapshots or keep
  only the latest value in application state.
