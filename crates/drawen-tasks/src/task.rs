use core::fmt;
use std::{
    any::Any,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{cancellation::CancellationToken, util::lock};

/// The completion slot shared by a [`TaskHandle`] and the pool worker running
/// the task.
pub(crate) struct Completion<T> {
    result: Mutex<Option<Result<T, String>>>,
    ready: Condvar,
    finished: AtomicBool,
}

impl<T> Completion<T> {
    pub(crate) fn new() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
            finished: AtomicBool::new(false),
        }
    }

    pub(crate) fn complete(&self, result: Result<T, String>) {
        *lock(&self.result) = Some(result);
        self.finished.store(true, Ordering::Release);
        self.ready.notify_all();
    }
}

/// An owned reference to a running or queued task.
///
/// Dropping the handle detaches from the task: the task keeps running and its
/// result is discarded. Call [`Self::join`] to wait for the result, or
/// [`Self::cancel`] to ask a cancellable task to stop early.
///
/// # Panics
///
/// [`Self::join`] panics only if the internal completion slot was corrupted;
/// a panic raised by the task itself is returned as
/// [`TaskJoinError::Panicked`].
pub struct TaskHandle<T> {
    completion: Arc<Completion<T>>,
    token: CancellationToken,
}

impl<T> TaskHandle<T> {
    pub(crate) fn new(completion: Arc<Completion<T>>, token: CancellationToken) -> Self {
        Self { completion, token }
    }

    /// Blocks the calling thread until the task finishes.
    ///
    /// This is appropriate for headless code and tests. A UI thread should
    /// instead poll [`Self::is_finished`] or deliver results through a
    /// [`UiDispatcher`](crate::UiDispatcher).
    ///
    /// # Errors
    ///
    /// Returns [`TaskJoinError::Panicked`] when the task panicked. The worker
    /// thread survives and remains available for other tasks.
    pub fn join(self) -> Result<T, TaskJoinError> {
        let mut slot = lock(&self.completion.result);
        while slot.is_none() {
            slot = self
                .completion
                .ready
                .wait(slot)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        match slot.take() {
            Some(Ok(value)) => Ok(value),
            Some(Err(message)) => Err(TaskJoinError::Panicked(message)),
            None => unreachable!("the slot is filled before the completion is signalled"),
        }
    }

    /// Returns whether the task has finished, successfully or not.
    ///
    /// A finished task still owns its result until [`Self::join`] consumes the
    /// handle.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.completion.finished.load(Ordering::Acquire)
    }

    /// Requests cooperative cancellation of this task.
    ///
    /// Tasks started with [`TaskPool::spawn`](crate::TaskPool::spawn) never
    /// observe the token, so this only affects tasks started with
    /// [`TaskPool::spawn_with_context`](crate::TaskPool::spawn_with_context) or
    /// [`TaskPool::spawn_with_progress`](crate::TaskPool::spawn_with_progress).
    /// Cancelling a queued task does not remove it from the queue; it starts
    /// and returns at its first cancellation checkpoint.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Returns the task's cancellation token.
    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}

impl<T> fmt::Debug for TaskHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskHandle")
            .field("finished", &self.is_finished())
            .field("cancelled", &self.token.is_cancelled())
            .finish_non_exhaustive()
    }
}

/// Why a task could not produce a value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskJoinError {
    /// The task panicked. Carries the panic message when it was a string.
    Panicked(String),
}

impl fmt::Display for TaskJoinError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panicked(message) => write!(formatter, "task panicked: {message}"),
        }
    }
}

impl std::error::Error for TaskJoinError {}

/// Extracts a human-readable message from a panic payload.
pub(crate) fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        String::from("task panicked with a non-string payload")
    }
}
