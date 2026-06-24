use crate::sync::SpinNoIrqLock as Mutex;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

bitflags! {
    pub struct Event: u32 {
        const READABLE                      = 1 << 0;
        const WRITABLE                      = 1 << 1;
        const ERROR                         = 1 << 2;
        const CLOSED                        = 1 << 3;
        const PROCESS_QUIT                  = 1 << 10;
        const CHILD_PROCESS_QUIT            = 1 << 11;
        const RECEIVE_SIGNAL                = 1 << 12;
        const SEMAPHORE_REMOVED             = 1 << 20;
        const SEMAPHORE_CAN_ACQUIRE         = 1 << 21;
    }
}

pub type EventHandler = Box<dyn Fn(Event) -> bool + Send>;

pub struct EventBus {
    event: Event,
    callbacks: Vec<EventHandler>,
}

impl Default for EventBus {
    fn default() -> Self {
        EventBus {
            event: Event::empty(),
            callbacks: Vec::new(),
        }
    }
}

impl EventBus {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn set(&mut self, set: Event) {
        self.change(Event::empty(), set);
    }

    pub fn clear(&mut self, set: Event) {
        self.change(set, Event::empty());
    }

    pub fn change(&mut self, reset: Event, set: Event) {
        let origin = self.event;
        let mut new = self.event;
        new.remove(reset);
        new.insert(set);
        self.event = new;
        // Only fire callbacks if the flags actually changed; drop the one-shot
        // callbacks (those returning `true`) that have fired.
        if new != origin {
            self.callbacks.retain(|f| !f(new));
        }
    }

    pub fn subscribe(&mut self, callback: EventHandler) {
        self.callbacks.push(callback);
    }

    pub fn get_callback_len(&self) -> usize {
        self.callbacks.len()
    }
}

/// Return a future that resolves once any event in `mask` is set on `bus`.
pub fn wait_for_event(bus: Arc<Mutex<EventBus>>, mask: Event) -> impl Future<Output = Event> {
    EventBusFuture { bus, mask }
}

#[must_use = "future does nothing unless polled/`await`-ed"]
struct EventBusFuture {
    bus: Arc<Mutex<EventBus>>,
    mask: Event,
}

impl Future for EventBusFuture {
    type Output = Event;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut lock = self.bus.lock();
        // Level check: if any requested event is already set, we're done.
        if !(lock.event & self.mask).is_empty() {
            return Poll::Ready(lock.event);
        }
        // Otherwise register a one-shot waker closure that fires when our mask
        // is signaled by a future `change()`.
        let waker = cx.waker().clone();
        let mask = self.mask;
        lock.subscribe(Box::new(move |s| {
            if (s & mask).is_empty() {
                return false;
            }
            waker.wake_by_ref();
            true
        }));
        Poll::Pending
    }
}
