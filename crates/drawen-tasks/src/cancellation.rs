use core::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Cooperative cancellation shared by a running task and its [`TaskHandle`].
///
/// A token never interrupts code or kills a thread. A task observes it at its
/// own checkpoints through [`Self::is_cancelled`] or
/// [`TaskContext::check_cancelled`](crate::TaskContext::check_cancelled), then
/// returns early. Dropping a [`TaskHandle`] does not cancel the task; call
/// [`TaskHandle::cancel`](crate::TaskHandle::cancel) explicitly.
///
/// [`TaskHandle`]: crate::TaskHandle
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates a token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation of every task sharing this token.
    ///
    /// The flag is set immediately and remains set. Cancellation is
    /// idempotent and can be requested from any thread.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Returns [`TaskCancelled`] once cancellation has been requested.
    ///
    /// # Errors
    ///
    /// Returns [`TaskCancelled`] when [`Self::cancel`] has been called.
    pub fn check(&self) -> Result<(), TaskCancelled> {
        if self.is_cancelled() {
            Err(TaskCancelled)
        } else {
            Ok(())
        }
    }
}

/// The cooperative cancellation signal returned by
/// [`CancellationToken::check`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskCancelled;

impl fmt::Display for TaskCancelled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task was cancelled")
    }
}

impl std::error::Error for TaskCancelled {}

#[cfg(test)]
mod tests {
    use super::CancellationToken;

    #[test]
    fn clones_share_cancellation() {
        let token = CancellationToken::new();
        let clone = token.clone();

        assert!(!token.is_cancelled());
        clone.cancel();

        assert!(token.is_cancelled());
        assert!(token.check().is_err());
    }
}
