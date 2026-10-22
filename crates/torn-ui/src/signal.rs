use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

type Listener<T> = Box<dyn FnMut(&T)>;
type ListenerSlot<T> = Rc<RefCell<Slot<T>>>;
type Listeners<T> = Rc<RefCell<Vec<Option<ListenerSlot<T>>>>>;

struct Slot<T> {
    callback: Option<Listener<T>>,
    connected: bool,
}

/// A drop-based connection to a [`Signal`].
///
/// Dropping the subscription disconnects its listener immediately, including
/// when this occurs from another listener during signal emission.
#[must_use = "dropping a subscription disconnects its listener immediately"]
pub struct Subscription {
    disconnect: Option<Box<dyn FnOnce()>>,
}

impl Subscription {
    /// Disconnects the listener now. Calling this more than once is harmless.
    pub fn unsubscribe(&mut self) {
        if let Some(disconnect) = self.disconnect.take() {
            disconnect();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

/// A typed synchronous notification channel owned by a widget or application model.
///
/// Cloning a signal creates another handle to the same listener list. Keep the
/// [`Subscription`] returned by [`Self::subscribe`] for as long as notifications
/// are wanted. Emission is safe when a callback subscribes, unsubscribes, or
/// emits the same signal again.
pub struct Signal<T> {
    listeners: Listeners<T>,
}

impl<T: 'static> Signal<T> {
    /// Creates a notification channel with no listeners.
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Adds a listener that receives each emitted value.
    pub fn subscribe(&self, listener: impl FnMut(&T) + 'static) -> Subscription {
        let slot = Rc::new(RefCell::new(Slot {
            callback: Some(Box::new(listener)),
            connected: true,
        }));
        let index = {
            let mut listeners = self.listeners.borrow_mut();
            if let Some(index) = listeners.iter().position(Option::is_none) {
                listeners[index] = Some(Rc::clone(&slot));
                index
            } else {
                listeners.push(Some(Rc::clone(&slot)));
                listeners.len() - 1
            }
        };
        let listeners = Rc::downgrade(&self.listeners);
        Subscription {
            disconnect: Some(Box::new(move || disconnect(&listeners, index, &slot))),
        }
    }

    /// Synchronously notifies all listeners registered when emission starts.
    pub fn emit(&self, value: &T) {
        let listeners: Vec<_> = self
            .listeners
            .borrow()
            .iter()
            .filter_map(|slot| slot.as_ref().map(Rc::clone))
            .collect();
        for slot in listeners {
            let callback = {
                let mut slot = slot.borrow_mut();
                if slot.connected {
                    slot.callback.take()
                } else {
                    None
                }
            };
            let Some(mut callback) = callback else {
                continue;
            };
            callback(value);
            let mut slot = slot.borrow_mut();
            if slot.connected {
                slot.callback = Some(callback);
            }
        }
    }

    /// Returns whether the channel has any connected listeners.
    #[must_use]
    pub fn has_listeners(&self) -> bool {
        self.listeners.borrow().iter().any(Option::is_some)
    }
}

fn disconnect<T>(
    listeners: &Weak<RefCell<Vec<Option<ListenerSlot<T>>>>>,
    index: usize,
    slot: &ListenerSlot<T>,
) {
    slot.borrow_mut().connected = false;
    let Some(listeners) = listeners.upgrade() else {
        return;
    };
    let mut listeners = listeners.borrow_mut();
    if listeners
        .get(index)
        .and_then(Option::as_ref)
        .is_some_and(|registered| Rc::ptr_eq(registered, slot))
    {
        listeners[index] = None;
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            listeners: Rc::clone(&self.listeners),
        }
    }
}

impl<T: 'static> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> core::fmt::Debug for Signal<T> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Signal")
            .field(
                "listener_count",
                &self
                    .listeners
                    .borrow()
                    .iter()
                    .filter(|slot| slot.is_some())
                    .count(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use super::Signal;

    #[test]
    fn cloned_handles_notify_the_same_listener_list() {
        let signal = Signal::new();
        let observed = Rc::new(Cell::new(0));
        let _subscription = signal.subscribe({
            let observed = Rc::clone(&observed);
            move |value: &u32| observed.set(observed.get() + value)
        });

        signal.clone().emit(&3);

        assert_eq!(observed.get(), 3);
    }

    #[test]
    fn dropping_a_subscription_disconnects_its_listener() {
        let signal = Signal::new();
        let observed = Rc::new(Cell::new(0));
        let subscription = signal.subscribe({
            let observed = Rc::clone(&observed);
            move |()| observed.set(observed.get() + 1)
        });

        assert!(signal.has_listeners());
        drop(subscription);
        signal.emit(&());

        assert!(!signal.has_listeners());
        assert_eq!(observed.get(), 0);
    }

    #[test]
    fn subscriptions_can_change_during_emission() {
        let signal = Signal::new();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let late_subscription = Rc::new(RefCell::new(None));
        let _first = signal.subscribe({
            let signal = signal.clone();
            let observed = Rc::clone(&observed);
            let late_subscription = Rc::clone(&late_subscription);
            move |value: &u32| {
                observed.borrow_mut().push(*value);
                if late_subscription.borrow().is_none() {
                    let observed = Rc::clone(&observed);
                    *late_subscription.borrow_mut() = Some(
                        signal.subscribe(move |value| observed.borrow_mut().push(*value + 10)),
                    );
                }
            }
        });

        signal.emit(&1);
        signal.emit(&2);

        assert_eq!(*observed.borrow(), vec![1, 2, 12]);
    }

    #[test]
    fn dropping_a_later_subscription_during_emission_skips_its_callback() {
        let signal = Signal::new();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let later_subscription = Rc::new(RefCell::new(None));
        let _first = signal.subscribe({
            let observed = Rc::clone(&observed);
            let later_subscription = Rc::clone(&later_subscription);
            move |value: &u32| {
                observed.borrow_mut().push(*value);
                let _ = later_subscription.borrow_mut().take();
            }
        });
        *later_subscription.borrow_mut() = Some(signal.subscribe({
            let observed = Rc::clone(&observed);
            move |value| observed.borrow_mut().push(*value + 10)
        }));

        signal.emit(&1);

        assert_eq!(*observed.borrow(), vec![1]);
    }

    #[test]
    fn reentrant_emission_does_not_borrow_the_listener_list_or_callback() {
        let signal = Signal::new();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let emitting = Rc::new(Cell::new(false));
        let _first = signal.subscribe({
            let signal = signal.clone();
            let observed = Rc::clone(&observed);
            let emitting = Rc::clone(&emitting);
            move |value: &u32| {
                observed.borrow_mut().push(*value);
                if !emitting.replace(true) {
                    signal.emit(&(*value + 1));
                }
                emitting.set(false);
            }
        });
        let _second = signal.subscribe({
            let observed = Rc::clone(&observed);
            move |value| observed.borrow_mut().push(*value * 10)
        });

        signal.emit(&1);

        assert_eq!(*observed.borrow(), vec![1, 20, 10]);
    }
}
