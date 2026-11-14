//! Worker threads, cancellable tasks, progress, and a UI-thread dispatcher.
//!
//! Drawen's widget tree, [`Signal`](https://docs.rs/drawen/latest/drawen/struct.Signal.html)s,
//! subscriptions, and [`UiRuntime`](https://docs.rs/drawen/latest/drawen/struct.UiRuntime.html)
//! are UI-thread-local. This crate provides the sanctioned boundary around that
//! rule:
//!
//! - [`TaskPool`] runs plain, owned, `Send` work on a fixed set of worker
//!   threads.
//! - [`TaskHandle`] joins a task, polls [`TaskHandle::is_finished`], or requests
//!   cooperative cancellation.
//! - [`TaskContext`] gives a running task its cancellation token and progress
//!   reporting.
//! - [`UiDispatcher`] moves owned results back to the UI thread and wakes the
//!   event loop.
//!
//! The rule stays unchanged: widgets and signals never cross a thread boundary.
//! Workers compute with owned data; the UI thread applies results to widgets.
//!
//! # Example
//!
//! ```
//! use drawen_tasks::{TaskPool, UiDispatcher};
//!
//! let pool = TaskPool::new(2).expect("worker threads start");
//! let (dispatcher, sender) = UiDispatcher::new(|| {
//!     // Wake the native event loop; see drawen-platform-winit.
//! });
//!
//! pool.spawn(move || sender.send(String::from("decoded")))
//!     .join()
//!     .expect("the task does not panic")
//!     .expect("the dispatcher is still alive");
//!
//! assert_eq!(dispatcher.drain(), vec![String::from("decoded")]);
//! ```
//!
//! # Scaling work
//!
//! ```
//! use drawen_tasks::TaskPool;
//!
//! let pool = TaskPool::new(4).expect("worker threads start");
//! let handles: Vec<_> = (0u64..100)
//!     .map(|value| pool.spawn(move || value * value))
//!     .collect();
//! let sum: u64 = handles
//!     .into_iter()
//!     .map(|handle| handle.join().expect("tasks do not panic"))
//!     .sum();
//!
//! assert_eq!(sum, (0u64..100).map(|value| value * value).sum());
//! ```

mod cancellation;
mod context;
mod dispatcher;
mod pool;
mod task;
mod util;

pub use cancellation::{CancellationToken, TaskCancelled};
pub use context::{TaskContext, TaskProgress};
pub use dispatcher::{UiDispatcher, UiSendError, UiSender};
pub use pool::TaskPool;
pub use task::{TaskHandle, TaskJoinError};
