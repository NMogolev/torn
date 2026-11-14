//! Integration tests for the task pool, cancellation, and progress.

use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use drawen_tasks::{TaskJoinError, TaskPool, TaskProgress};

#[test]
fn runs_tasks_and_returns_their_results() {
    let pool = TaskPool::new(3).expect("workers start");
    let handles: Vec<_> = (0u64..32)
        .map(|value| pool.spawn(move || value * value))
        .collect();
    let sum: u64 = handles
        .into_iter()
        .map(|handle| handle.join().expect("no task panics"))
        .sum();

    assert_eq!(sum, (0u64..32).map(|value| value * value).sum());
}

#[test]
fn limits_concurrency_to_the_worker_count() {
    let pool = TaskPool::new(3).expect("workers start");
    let barrier = Arc::new(Barrier::new(3));
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..3 {
        let barrier = Arc::clone(&barrier);
        let running = Arc::clone(&running);
        let peak = Arc::clone(&peak);
        handles.push(pool.spawn(move || {
            let current = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            // Only returns once all three workers are inside their task.
            barrier.wait();
            running.fetch_sub(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.join().expect("no task panics");
    }

    assert_eq!(peak.load(Ordering::SeqCst), 3);
}

#[test]
fn cancellation_reaches_a_running_task() {
    let pool = TaskPool::new(1).expect("workers start");
    let handle = pool.spawn_with_context(|context| {
        while !context.is_cancelled() {
            thread::yield_now();
        }
        17
    });

    handle.cancel();

    assert_eq!(
        handle
            .join()
            .expect("a cancelled task still returns its value"),
        17
    );
}

#[test]
fn reports_progress_from_the_worker_thread() {
    let pool = TaskPool::new(1).expect("workers start");
    let (sender, receiver) = mpsc::channel();
    let handle = pool.spawn_with_progress(
        |context| {
            context.report_units(1, 2);
            context.report_units(2, 2);
            "done"
        },
        move |progress| {
            sender.send(progress).expect("test receiver remains open");
        },
    );

    assert_eq!(handle.join().expect("no task panics"), "done");

    let reported: Vec<_> = receiver.try_iter().collect();
    assert_eq!(
        reported,
        vec![
            TaskProgress::with_total(1, 2),
            TaskProgress::with_total(2, 2)
        ]
    );
}

#[test]
fn captures_a_task_panic_without_killing_the_worker() {
    let pool = TaskPool::new(1).expect("workers start");
    let panicked = pool.spawn(|| -> u8 { panic!("boom") });

    assert!(matches!(
        panicked.join(),
        Err(TaskJoinError::Panicked(message)) if message.contains("boom")
    ));
    assert_eq!(pool.spawn(|| 5).join().expect("worker survives"), 5);
}

#[test]
fn shutdown_finishes_already_queued_jobs() {
    let pool = TaskPool::new(1).expect("workers start");
    let (sender, receiver) = mpsc::channel();
    pool.spawn(move || {
        thread::sleep(Duration::from_millis(20));
        sender.send("finished").expect("test receiver remains open");
    });

    pool.shutdown();

    assert_eq!(receiver.try_recv(), Ok("finished"));
}

#[test]
fn active_and_queued_counts_track_the_queue() {
    let pool = TaskPool::new(1).expect("workers start");
    let (sender, receiver) = mpsc::channel();
    let block = {
        let sender = sender.clone();
        move || {
            let _ = sender.send(());
            thread::sleep(Duration::from_millis(30));
        }
    };

    drop(pool.spawn(block));
    for _ in 0..4 {
        drop(pool.spawn({
            let sender = sender.clone();
            move || sender.send(()).expect("test receiver remains open")
        }));
    }

    receiver.recv().expect("first task starts");
    assert!(pool.active_tasks() >= 1);
    assert!(pool.queued_tasks() > 0);

    pool.shutdown();
}
