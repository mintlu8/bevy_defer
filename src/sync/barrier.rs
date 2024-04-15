//! A barrier enables multiple tasks to synchronize the beginning of some computation.

use std::{cell::Cell, pin::Pin, rc::Rc, task::{Context, Poll}};
use futures::Future;
use super::waitlist::WaitList;

/// A barrier enables multiple tasks to synchronize the beginning of some computation.
#[derive(Debug, Clone)]
pub struct Barrier{
    inner: Rc<BarrierInner>,
}

#[derive(Debug)]
struct BarrierInner {
    permits: Cell<u64>,
    wait_list: WaitList,
}

/// [`Future`] for a [`Barrier`].
#[must_use = "Future does nothing util polled."]
struct BarrierFuture {
    barrier: Rc<BarrierInner>,
}

impl Future for BarrierFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.barrier.permits.get() == 0 {
            Poll::Ready(())
        } else {
            self.barrier.wait_list.push_cx(cx);
            Poll::Pending
        }
    }
}

impl Barrier {
    pub fn new(permits: u64) -> Barrier {
        Barrier { 
            inner: Rc::new(BarrierInner {
                permits: Cell::new(permits),
                wait_list: Default::default(),
            }),
        }
    }

    pub async fn wait(&self) {
        let value = self.inner.permits.get().saturating_sub(1);
        self.inner.permits.set(value);
        if value == 0 {
            self.inner.wait_list.wake();
        } else {
            BarrierFuture { barrier: self.inner.clone() }.await
        }
    }
}
