use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, recovering the data if another thread panicked while holding
/// it.
///
/// Drawen tasks capture panics before they reach a worker loop, and the
/// dispatcher and completion state contain no user code that can leave them in
/// an inconsistent state. A poisoned lock therefore still holds valid data.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
