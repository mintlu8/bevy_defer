use std::{ops::Deref, sync::atomic::AtomicU32, task::Poll};
use triomphe::Arc;

use std::sync::atomic::Ordering;
use std::future::Future;
use parking_lot::Mutex;

use crate::{AsObject, Object};

use super::TypedSignal;

/// The data component of a signal.
#[derive(Debug, Default)]
pub struct SignalData<T> {
    data: Mutex<T>,
    version: AtomicU32,
}

/// The shared component of a signal.
#[derive(Debug, Default)]
pub struct SignalInner<T> {
    inner: Arc<SignalData<T>>,
    version: AtomicU32,
}

/// A piece of shared data that contains a version number for synchronization.
#[derive(Debug, Default)]
pub struct Signal<T> {
    pub(super) inner: Arc<SignalInner<T>>
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal {
            inner: Arc::new(SignalInner {
                inner: self.inner.inner.clone(),
                version: AtomicU32::new(self.inner.version.load(Ordering::Relaxed))
            })
        }
    }
}

pub(crate) struct YieldNow(bool);

impl YieldNow {
    pub fn new() -> Self {
        Self(false)
    }
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        Poll::Pending
    }
}

impl<T: Send + Sync + 'static> Signal<T> {
    pub fn new(value: T) -> Self{
        Self {inner: Arc::new(SignalInner {
            inner: Arc::new(SignalData {
                data: Mutex::new(value),
                version: AtomicU32::new(0),
            }),
            version: AtomicU32::new(0),
        })}
    }

    pub fn borrow_inner(&self) -> Arc<SignalInner<T>> {
        self.inner.clone()
    }

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

impl Signal<Object> {
    pub fn from_typed<T: AsObject>(value: TypedSignal<T>) -> Self{
        Self {inner: Arc::new(SignalInner {
            inner: value.into_inner(),
            version: AtomicU32::new(0),
        })}
    }
}

impl<T: Send + Sync + 'static> SignalInner<T> {
    /// Note: This does not increment the read counter.
    pub fn write(&self, value: T) {
        let mut lock = self.inner.data.lock();
        *lock = value;
        self.inner.version.fetch_add(1, Ordering::Relaxed);
        drop(lock);
    }

    /// Note: This does not increment the read counter.
    pub fn write_if_changed(&self, value: T) where T: PartialEq {
        let mut lock = self.inner.data.lock();
        if *lock != value {
            *lock = value;
            drop(lock);
            self.inner.version.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// This increases the read counter.
    pub fn broadcast(&self, value: T) {
        let mut lock = self.inner.data.lock();
        *lock = value;
        let version = self.inner.version.fetch_add(1, Ordering::Relaxed);
        drop(lock);
        self.version.store(version.wrapping_add(1), Ordering::Relaxed)
    }


    /// Note: This does not increment the read counter.
    pub fn broadcast_if_changed(&self, value: T) where T: PartialEq {
        let mut lock = self.inner.data.lock();
        if *lock != value {
            *lock = value;
            let version = self.inner.version.fetch_add(1, Ordering::Relaxed);
            drop(lock);
            self.version.store(version.wrapping_add(1), Ordering::Relaxed)
        }
    }

    pub fn try_read(&self) -> Option<T> where T: Clone {
        let version = self.inner.version.load(Ordering::Relaxed);
        if self.version.swap(version, Ordering::Relaxed) != version {
            Some(self.inner.data.lock().clone())
        } else {
            None
        }
    }

    /// Reads the underlying value regardless of change detection.
    pub fn force_read(&self) -> T where T: Clone {
        let version = self.inner.version.load(Ordering::Relaxed);
        self.version.swap(version, Ordering::Relaxed);
        self.inner.data.lock().clone()
    }

    pub async fn async_read(&self) -> T where T: Clone {
        loop {
            let version = self.inner.version.load(Ordering::Relaxed);
            if self.version.swap(version, Ordering::Relaxed) != version {
                return self.inner.data.lock().clone();
            } else {
                YieldNow::new().await;
            }
        }
    }

    pub fn get_shared(&self) -> Arc<SignalData<T>> {
        self.inner.clone()
    }
}


impl SignalInner<Object> {
    pub fn try_read_as<T: AsObject>(&self) -> Option<T> where T: Clone {
        let version = self.inner.version.load(Ordering::Relaxed);
        if self.version.swap(version, Ordering::Relaxed) != version {
            Some(self.inner.data.lock().clone())
                .and_then(|x| x.get())
        } else {
            None
        }
    }
}