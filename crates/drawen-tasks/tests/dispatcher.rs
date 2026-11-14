//! Integration tests for the worker-to-UI dispatcher.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use drawen_tasks::{TaskPool, UiDispatcher, UiSendError, UiSender};

fn assert_send_sync<T: Send + Sync>() {}
fn assert_send<T: Send>() {}

#[test]
fn worker_messages_are_delivered_and_wake_the_ui() {
    assert_send_sync::<UiSender<u32>>();
    assert_send::<UiDispatcher<u32>>();

    let wakes = Arc::new(AtomicUsize::new(0));
    let (dispatcher, sender) = {
        let wakes = Arc::clone(&wakes);
        UiDispatcher::new(move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
    };

    sender.send(1).expect("dispatcher is alive");
    sender.send(2).expect("dispatcher is alive");

    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    assert_eq!(dispatcher.len(), 2);
    assert_eq!(dispatcher.drain(), vec![1, 2]);
    assert!(dispatcher.is_empty());
}

#[test]
fn dispatcher_drains_values_produced_by_a_pool_task() {
    let pool = TaskPool::new(2).expect("workers start");
    let (dispatcher, sender) = UiDispatcher::new(|| {});

    let handle = pool.spawn(move || {
        for value in 0u32..4 {
            sender.send(value).expect("dispatcher is alive");
        }
    });
    handle.join().expect("no task panics");

    assert_eq!(dispatcher.drain(), vec![0, 1, 2, 3]);
}

#[test]
fn send_returns_the_value_after_the_dispatcher_is_dropped() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let (dispatcher, sender) = {
        let wakes = Arc::clone(&wakes);
        UiDispatcher::new(move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
    };
    drop(dispatcher);

    let error: UiSendError<String> = sender
        .send(String::from("late"))
        .expect_err("UI side is gone");

    assert_eq!(error.into_inner(), "late");
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
}

#[test]
fn drain_into_appends_to_an_existing_buffer() {
    let (dispatcher, sender) = UiDispatcher::new(|| {});
    sender.send(2).expect("dispatcher is alive");
    sender.send(3).expect("dispatcher is alive");

    let mut values = vec![1];
    dispatcher.drain_into(&mut values);

    assert_eq!(values, vec![1, 2, 3]);
}

#[test]
fn sender_obtained_from_the_dispatcher_keeps_the_channel_open() {
    let (dispatcher, sender) = UiDispatcher::new(|| {});
    drop(sender);

    dispatcher
        .sender()
        .send(9)
        .expect("the dispatcher keeps its own sender");

    assert_eq!(dispatcher.drain(), vec![9]);
}
