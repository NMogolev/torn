use std::{cell::RefCell, rc::Rc};

type Listener<T> = Box<dyn FnMut(&T)>;
type Listeners<T> = Rc<RefCell<Vec<Listener<T>>>>;

/// A typed synchronous notification channel owned by a widget or application model.
///
/// Cloning a signal creates another handle to the same listener list. Listeners are
/// retained for the lifetime of the signal; keep subscriptions small and move
/// application state into their closures.
pub struct Signal<T> {
    listeners: Listeners<T>,
}

impl<T> Signal<T> {
    /// Creates a notification channel with no listeners.
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Adds a listener that receives each emitted value.
    pub fn subscribe(&self, listener: impl FnMut(&T) + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    /// Synchronously notifies all registered listeners.
    pub fn emit(&self, value: &T) {
        for listener in self.listeners.borrow_mut().iter_mut() {
            listener(value);
        }
    }

    /// Returns whether the channel has any listeners.
    #[must_use]
    pub fn has_listeners(&self) -> bool {
        !self.listeners.borrow().is_empty()
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            listeners: Rc::clone(&self.listeners),
        }
    }
}

impl<T> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> core::fmt::Debug for Signal<T> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Signal")
            .field("listener_count", &self.listeners.borrow().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::Signal;

    #[test]
    fn cloned_handles_notify_the_same_listener_list() {
        let signal = Signal::new();
        let observed = Rc::new(Cell::new(0));
        signal.subscribe({
            let observed = Rc::clone(&observed);
            move |value: &u32| observed.set(observed.get() + value)
        });

        signal.clone().emit(&3);

        assert_eq!(observed.get(), 3);
    }
}
