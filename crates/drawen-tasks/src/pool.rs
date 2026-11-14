use core::fmt;
use std::{
    collections::VecDeque,
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

use crate::{
    cancellation::CancellationToken,
    context::{ProgressCallback, TaskContext, TaskProgress},
    task::{Completion, TaskHandle, panic_message},
    util::lock,
};

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Queue {
    jobs: VecDeque<Job>,
    active: usize,
    shutdown: bool,
}

impl Queue {
    fn new() -> Self {
        Self {
            jobs: VecDeque::new(),
            active: 0,
            shutdown: false,
        }
    }
}

struct Shared {
    queue: Mutex<Queue>,
    available: Condvar,
    worker_count: usize,
}

/// A fixed-size pool of worker threads that run plain, owned work.
///
/// The pool is the sanctioned place for decoding, parsing, hashing, network
/// payload handling, and other blocking work that must stay off the UI thread.
/// It never touches the widget tree: tasks own their inputs, return owned
/// values, and hand results back through a
/// [`UiDispatcher`](crate::UiDispatcher).
///
/// The pool is `Send + Sync`, so one pool can serve several windows. Its
/// worker count is fixed at construction; `new` clamps zero to one. Dropping or
/// calling [`Self::shutdown`] stops accepting work, finishes jobs that are
/// already queued, and joins every worker.
///
/// # Panics
///
/// A panic inside a task is caught and reported through
/// [`TaskHandle::join`] as [`TaskJoinError::Panicked`]; the worker survives.
/// Dropping the pool from one of its own tasks deadlocks, because shutdown
/// waits for that task to finish.
///
/// [`TaskJoinError::Panicked`]: crate::TaskJoinError::Panicked
pub struct TaskPool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl TaskPool {
    /// Starts a pool with `worker_count` threads.
    ///
    /// # Errors
    ///
    /// Returns the operating-system error if a worker thread cannot be
    /// created. Threads that were already started are stopped before the error
    /// is returned.
    pub fn new(worker_count: usize) -> io::Result<Self> {
        Self::with_thread_name(worker_count, "drawen-task")
    }

    /// Starts a pool whose threads are named `{name}-{index}`.
    ///
    /// # Errors
    ///
    /// Returns the operating-system error if a worker thread cannot be
    /// created. Threads that were already started are stopped before the error
    /// is returned.
    pub fn with_thread_name(worker_count: usize, name: &str) -> io::Result<Self> {
        let worker_count = worker_count.max(1);
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::new()),
            available: Condvar::new(),
            worker_count,
        });
        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let worker_shared = Arc::clone(&shared);
            let spawned = thread::Builder::new()
                .name(format!("{name}-{index}"))
                .spawn(move || work(&worker_shared));
            match spawned {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    lock(&shared.queue).shutdown = true;
                    shared.available.notify_all();
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self { shared, workers })
    }

    /// Starts a pool with one worker per available hardware thread.
    ///
    /// # Errors
    ///
    /// Returns the operating-system error if a worker thread cannot be
    /// created.
    pub fn for_available_parallelism() -> io::Result<Self> {
        let worker_count = thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self::new(worker_count)
    }

    /// Queues an owned closure and returns a handle to its result.
    ///
    /// The closure runs on a worker thread and must not capture widgets,
    /// [`Signal`](https://docs.rs/drawen/latest/drawen/struct.Signal.html)s,
    /// subscriptions, or a [`UiRuntime`](https://docs.rs/drawen/latest/drawen/struct.UiRuntime.html).
    pub fn spawn<F, T>(&self, task: F) -> TaskHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.spawn_with_context(|_context| task())
    }

    /// Queues a closure that receives a [`TaskContext`] for cancellation
    /// checks.
    ///
    /// The returned handle can cancel the task through
    /// [`TaskHandle::cancel`].
    pub fn spawn_with_context<F, T>(&self, task: F) -> TaskHandle<T>
    where
        F: FnOnce(TaskContext) -> T + Send + 'static,
        T: Send + 'static,
    {
        self.enqueue(task, None)
    }

    /// Queues a closure that receives a [`TaskContext`] and reports progress
    /// through `on_progress`.
    ///
    /// `on_progress` runs on the worker thread. Keep it cheap and forward the
    /// snapshot through a [`UiDispatcher`](crate::UiDispatcher) or another
    /// channel; never mutate UI state from it.
    pub fn spawn_with_progress<F, T, P>(&self, task: F, on_progress: P) -> TaskHandle<T>
    where
        F: FnOnce(TaskContext) -> T + Send + 'static,
        T: Send + 'static,
        P: Fn(TaskProgress) + Send + Sync + 'static,
    {
        self.enqueue(task, Some(Arc::new(on_progress)))
    }

    /// Returns the fixed number of worker threads.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.shared.worker_count
    }

    /// Returns the number of jobs waiting for a free worker.
    #[must_use]
    pub fn queued_tasks(&self) -> usize {
        lock(&self.shared.queue).jobs.len()
    }

    /// Returns the number of jobs currently running.
    #[must_use]
    pub fn active_tasks(&self) -> usize {
        lock(&self.shared.queue).active
    }

    /// Returns whether the queue is empty and every worker is idle.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        let queue = lock(&self.shared.queue);
        queue.jobs.is_empty() && queue.active == 0
    }

    /// Stops the pool, finishes already-queued jobs, and joins every worker.
    ///
    /// This is equivalent to dropping the pool, but makes the wait explicit.
    pub fn shutdown(self) {
        drop(self);
    }

    fn enqueue<F, T>(&self, task: F, progress: Option<ProgressCallback>) -> TaskHandle<T>
    where
        F: FnOnce(TaskContext) -> T + Send + 'static,
        T: Send + 'static,
    {
        let token = CancellationToken::new();
        let completion = Arc::new(Completion::new());
        let worker_completion = Arc::clone(&completion);
        let context = TaskContext::new(token.clone(), progress);
        let job: Job = Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(|| task(context)))
                .map_err(|payload| panic_message(payload.as_ref()));
            worker_completion.complete(result);
        });
        self.push(job);
        TaskHandle::new(completion, token)
    }

    fn push(&self, job: Job) {
        let mut queue = lock(&self.shared.queue);
        debug_assert!(
            !queue.shutdown,
            "a TaskPool cannot accept work after shutdown"
        );
        queue.jobs.push_back(job);
        drop(queue);
        self.shared.available.notify_one();
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        lock(&self.shared.queue).shutdown = true;
        self.shared.available.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl fmt::Debug for TaskPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let queue = lock(&self.shared.queue);
        formatter
            .debug_struct("TaskPool")
            .field("workers", &self.shared.worker_count)
            .field("queued", &queue.jobs.len())
            .field("active", &queue.active)
            .field("shutdown", &queue.shutdown)
            .finish()
    }
}

fn work(shared: &Shared) {
    loop {
        let Some(job) = next_job(shared) else {
            break;
        };
        job();
        let mut queue = lock(&shared.queue);
        queue.active -= 1;
        let wake_waiters = queue.shutdown && queue.jobs.is_empty();
        drop(queue);
        if wake_waiters {
            shared.available.notify_all();
        }
    }
}

fn next_job(shared: &Shared) -> Option<Job> {
    let mut queue = lock(&shared.queue);
    loop {
        if let Some(job) = queue.jobs.pop_front() {
            queue.active += 1;
            return Some(job);
        }
        if queue.shutdown {
            return None;
        }
        queue = shared
            .available
            .wait(queue)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}
