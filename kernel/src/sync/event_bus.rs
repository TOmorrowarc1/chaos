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
        todo!()
    }

    pub fn clear(&mut self, set: Event) {
        todo!()
    }

    pub fn change(&mut self, reset: Event, set: Event) {
        todo!()
    }

    pub fn subscribe(&mut self, callback: EventHandler) {
        todo!()
    }

    pub fn get_callback_len(&self) -> usize {
        todo!()
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
        todo!()
    }
}
