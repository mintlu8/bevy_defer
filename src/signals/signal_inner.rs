use std::fmt::Debug;
use std::task::Waker;
use std::{ops::Deref, sync::atomic::AtomicU32, task::Poll};
use std::sync::Arc;

use std::sync::atomic::Ordering;
use std::future::poll_fn;
use futures::Future;
use parking_lot::Mutex;

/// The data component of a signal.
#[derive(Debug, Default)]
pub struct SignalData<T> {
    pub(crate) data: Mutex<T>,
    pub(crate) tick: AtomicU32,
    pub(crate) wakers: Mutex<Vec<Waker>>
}

/// The shared component of a signal.
#[derive(Debug, Default)]
pub struct SignalInner<T> {
    pub(crate) inner: Arc<SignalData<T>>,
    pub(crate) tick: AtomicU32,
}

/// A piece of shared data that can be read once per write.
#[derive(Debug, Default)]
pub struct Signal<T> {
    pub(super) inner: Arc<SignalInner<T>>
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal {
            inner: Arc::new(SignalInner {
                inner: self.inner.inner.clone(),
                tick: AtomicU32::new(self.inner.tick.load(Ordering::Relaxed))
            })
        }
    }
}

impl<T: Send + Sync + 'static> Signal<T> {
    /// Create a new signal.
    pub fn new(value: T) -> Self{
        Self {inner: Arc::new(SignalInner {
            inner: Arc::new(SignalData {
                data: Mutex::new(value),
                tick: AtomicU32::new(0),
                wakers: Default::default(),
            }),
            tick: AtomicU32::new(0),
        })}
    }

    /// Borrow the inner value with shared read tick.
    pub fn borrow_inner(&self) -> Arc<SignalInner<T>> {
        self.inner.clone()
    }

    /// Reference to the inner value with shared read tick.
    pub fn inner(&self) -> &Arc<SignalInner<T>> {
        &self.inner
    }
}

impl<T: Send + Sync + 'static> Deref for Signal<T> {
    type Target = Arc<SignalInner<T>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> From<Arc<SignalData<T>>> for Signal<T>{
    fn from(value: Arc<SignalData<T>>) -> Self {
        Self {inner: Arc::new(SignalInner::from(value))}
    }
}

impl<T> From<Arc<SignalData<T>>> for SignalInner<T>{
    fn from(value:Arc<SignalData<T>>) -> Self {
        Self {
            tick: AtomicU32::new(value.tick.load(Ordering::Relaxed)),
            inner: value,
        }
    }
}

impl<T: Send + Sync + 'static> SignalInner<T> {
    /// Note: This does not increment the read counter.
    pub fn write(&self, value: T) {
        let mut lock = self.inner.data.lock();
        *lock = value;
        self.inner.tick.fetch_add(1, Ordering::Relaxed);
        let mut wakers = self.inner.wakers.lock();
        wakers.drain(..).for_each(|x| x.wake());
    }

    /// Note: This does not increment the read counter.
    pub fn write_if_changed(&self, value: T) where T: PartialEq {
        let mut lock = self.inner.data.lock();
        if *lock != value {
            *lock = value;
            let mut wakers = self.inner.wakers.lock();
            wakers.drain(..).for_each(|x| x.wake());
            self.inner.tick.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// This increases the read counter.
    pub fn broadcast(&self, value: T) {
        let mut lock = self.inner.data.lock();
        *lock = value;
        let version = self.inner.tick.fetch_add(1, Ordering::Relaxed);
        let mut wakers = self.inner.wakers.lock();
        wakers.drain(..).for_each(|x| x.wake());
        self.tick.store(version.wrapping_add(1), Ordering::Relaxed)
    }


    /// Note: This does not increment the read counter.
    pub fn broadcast_if_changed(&self, value: T) where T: PartialEq {
        let mut lock = self.inner.data.lock();
        if *lock != value {
            *lock = value;
            let version = self.inner.tick.fetch_add(1, Ordering::Relaxed);
            let mut wakers = self.inner.wakers.lock();
            wakers.drain(..).for_each(|x| x.wake());
            self.tick.store(version.wrapping_add(1), Ordering::Relaxed)
        }
    }

    /// Reads the underlying value synchronously if changed.
    pub fn try_read(&self) -> Option<T> where T: Clone {
        let version = self.inner.tick.load(Ordering::Relaxed);
        if self.tick.swap(version, Ordering::Relaxed) != version {
            Some(self.inner.data.lock().clone())
        } else {
            None
        }
    }

    /// Reads the underlying value synchronously regardless of change detection.
    pub fn force_read(&self) -> T where T: Clone {
        let version = self.inner.tick.load(Ordering::Relaxed);
        self.tick.swap(version, Ordering::Relaxed);
        self.inner.data.lock().clone()
    }

    /// Reads the underlying value asynchronously when changed.
    pub fn async_read(self: &Arc<Self>) -> impl Future<Output = T> where T: Clone {
        let this = self.clone();
        async move {
            let mut first = true;
            loop {
                let tick = this.inner.tick.load(Ordering::Relaxed);
                if this.tick.swap(tick, Ordering::Relaxed) != tick {
                    return this.inner.data.lock().clone();
                } else if first {
                    first = false;
                    let waker = poll_fn(|ctx| Poll::Ready(ctx.waker().clone())).await;
                    let mut lock = this.inner.wakers.lock();
                    lock.push(waker);
                }
                let mut yielded = false;
                poll_fn(|_| if yielded {
                    Poll::Ready(())
                } else {
                    yielded = true;
                    Poll::Pending
                }).await
            }
        }
    }
}
