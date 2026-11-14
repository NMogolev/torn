use core::fmt;
use std::sync::Arc;

use crate::cancellation::{CancellationToken, TaskCancelled};

/// A progress snapshot reported by a running task.
///
/// Progress is advisory: the callback runs on the worker thread, and the UI
/// thread decides whether and when to repaint. A task that reports
/// [`Self::with_total`] can also be rendered as a determinate
/// [`ProgressBar`](https://docs.rs/drawen/latest/drawen/struct.ProgressBar.html).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskProgress {
    completed: u64,
    total: Option<u64>,
}

impl TaskProgress {
    /// Creates a snapshot from completed units and an optional total.
    #[must_use]
    pub const fn new(completed: u64, total: Option<u64>) -> Self {
        Self { completed, total }
    }

    /// Creates a snapshot with a known total.
    #[must_use]
    pub const fn with_total(completed: u64, total: u64) -> Self {
        Self::new(completed, Some(total))
    }

    /// Returns how many units the task has completed.
    #[must_use]
    pub const fn completed(&self) -> u64 {
        self.completed
    }

    /// Returns the total unit count, when the task knows it.
    #[must_use]
    pub const fn total(&self) -> Option<u64> {
        self.total
    }

    /// Returns the completed fraction clamped to `0.0..=1.0`.
    ///
    /// Returns `None` when the total is unknown or zero.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn fraction(&self) -> Option<f32> {
        self.total
            .filter(|total| *total > 0)
            .map(|total| self.completed.min(total) as f32 / total as f32)
    }

    /// Returns whether the completed count reached a known total.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.total.is_some_and(|total| self.completed >= total)
    }
}

pub(crate) type ProgressCallback = Arc<dyn Fn(TaskProgress) + Send + Sync>;

/// The cooperative services available to a task started with
/// [`TaskPool::spawn_with_context`](crate::TaskPool::spawn_with_context) or
/// [`TaskPool::spawn_with_progress`](crate::TaskPool::spawn_with_progress).
///
/// The context is owned by the task, carries no UI state, and is deliberately
/// usable only from the worker thread that runs the task.
pub struct TaskContext {
    token: CancellationToken,
    progress: Option<ProgressCallback>,
}

impl TaskContext {
    pub(crate) fn new(token: CancellationToken, progress: Option<ProgressCallback>) -> Self {
        Self { token, progress }
    }

    /// Returns the cancellation token shared with the [`TaskHandle`].
    ///
    /// [`TaskHandle`]: crate::TaskHandle
    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Returns whether the caller has requested cancellation.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Returns [`TaskCancelled`] once the caller has requested cancellation.
    ///
    /// # Errors
    ///
    /// Returns [`TaskCancelled`] after
    /// [`TaskHandle::cancel`](crate::TaskHandle::cancel) or
    /// [`CancellationToken::cancel`] has been called.
    pub fn check_cancelled(&self) -> Result<(), TaskCancelled> {
        self.token.check()
    }

    /// Reports a progress snapshot to the callback supplied at spawn time.
    ///
    /// This is a no-op for tasks started without a progress callback. The
    /// callback runs on the worker thread, so it should be cheap: forward the
    /// snapshot through a channel instead of touching widgets or signals.
    pub fn report(&self, progress: TaskProgress) {
        if let Some(callback) = &self.progress {
            callback(progress);
        }
    }

    /// Reports completed and total units as a single snapshot.
    pub fn report_units(&self, completed: u64, total: u64) {
        self.report(TaskProgress::with_total(completed, total));
    }
}

impl fmt::Debug for TaskContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskContext")
            .field("cancelled", &self.is_cancelled())
            .field("reports_progress", &self.progress.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::TaskProgress;

    #[test]
    fn fractions_are_clamped_and_ignore_unknown_totals() {
        assert_eq!(TaskProgress::new(3, Some(4)).fraction(), Some(0.75));
        assert_eq!(TaskProgress::new(9, Some(4)).fraction(), Some(1.0));
        assert_eq!(TaskProgress::new(3, None).fraction(), None);
        assert_eq!(TaskProgress::new(3, Some(0)).fraction(), None);
        assert!(TaskProgress::with_total(4, 4).is_complete());
    }
}
