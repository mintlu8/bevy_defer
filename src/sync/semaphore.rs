//! Counting semaphore performing asynchronous permit acquisition.

use std::{cell::{Cell, RefCell}, pin::Pin, rc::Rc, task::{Context, Poll, Waker}};
use futures::Future;

/// Counting semaphore performing asynchronous permit acquisition.
#[derive(Debug, Clone)]
pub struct Semaphore{
    inner: Rc<SemaphoreInner>,
    total: u64,
}

#[derive(Debug)]
struct SemaphoreInner {
    permits: Cell<u64>,
    waiters: Rc<RefCell<Vec<Waker>>>,
}

/// [`Future`] for obtaining a [`SemaphorePermit`].
#[must_use = "Future does nothing util polled."]
pub struct SemaphoreFuture {
    semaphore: Rc<SemaphoreInner>,
    permits: u64,
}

impl Future for SemaphoreFuture {
    type Output = SemaphorePermit;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.semaphore.permits.get() >= self.permits {
            self.semaphore.permits.set(self.semaphore.permits.get() - self.permits);
            Poll::Ready(SemaphorePermit {
                semaphore: self.semaphore.clone(),
                permits: self.permits
            })
        } else {
            self.semaphore.waiters.borrow_mut().push(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// A owned permit of a [`Semaphore`].
#[must_use = "SemaphorePermit must be held."]
pub struct SemaphorePermit {
    semaphore: Rc<SemaphoreInner>,
    permits: u64,
}

impl Drop for SemaphorePermit {
    fn drop(&mut self) {
        self.semaphore.permits.set(self.semaphore.permits.get() + self.permits);
        self.semaphore.waiters.borrow_mut().drain(..).for_each(|w| w.wake())
    }
}

impl Semaphore {
    pub fn new(permits: u64) -> Semaphore {
        Semaphore { 
            inner: Rc::new(SemaphoreInner {
                permits: Cell::new(permits),
                waiters: Default::default(),
            }),
            total: permits
        }
    }

    pub fn acquire(&self) -> SemaphoreFuture {
        SemaphoreFuture {
            semaphore: self.inner.clone(),
            permits: 1,
        }
    }

    pub fn acquire_many(&self, permits: u64) -> SemaphoreFuture {
        SemaphoreFuture {
            semaphore: self.inner.clone(),
            permits,
        }
    }

    pub fn acquire_all(&self) -> SemaphoreFuture {
        SemaphoreFuture {
            semaphore: self.inner.clone(),
            permits: self.total,
        }
    }
}
