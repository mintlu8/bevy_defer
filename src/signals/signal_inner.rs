use std::convert::Infallible;
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Waker};
use std::{ops::Deref, sync::atomic::AtomicU32, task::Poll};

use futures::future::FusedFuture;
use futures::{Future, Sink, Stream};
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::Ordering;

/// The data component of a signal.
#[derive(Debug, Default)]
pub(crate) struct SignalData<T> {
    pub(crate) data: RwLock<T>,
    pub(crate) tick: AtomicU32,
    pub(crate) wakers: Mutex<Vec<Waker>>,
}

/// The shared component of a signal.
///
/// `Arc<SignalInner<T>>` is a clone of a [`Signal`] that shares the read tick,
/// compared to calling `clone` on a signal.
#[derive(Debug, Default)]
pub struct SignalInner<T> {
    pub(crate) inner: Arc<SignalData<T>>,
    pub(crate) tick: AtomicU32,
}

/// A piece of shared data that can be read once per write.
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct Signal<T>(Arc<SignalInner<T>>);

/// A piece of shared data that can be read once per write.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct SignalBorrow<T>(Arc<SignalInner<T>>);

impl<T> Deref for SignalBorrow<T> {
    type Target = Arc<SignalInner<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal(Arc::new(SignalInner {
            inner: self.0.inner.clone(),
            tick: AtomicU32::new(self.0.tick.load(Ordering::Relaxed)),
        }))
    }
}

impl<T: Send + Sync + 'static> Signal<T> {
    /// Create a new signal.
    pub fn new(value: T) -> Self {
        Self(Arc::new(SignalInner {
            inner: Arc::new(SignalData {
                data: RwLock::new(value),
                tick: AtomicU32::new(0),
                wakers: Default::default(),
            }),
            tick: AtomicU32::new(0),
        }))
    }

    /// Borrow the inner value with shared read tick.
    pub fn borrow_inner(&self) -> SignalBorrow<T> {
        SignalBorrow(self.0.clone())
    }

    /// Rewind the read tick and allow the current value to be read.
    pub fn rewind(&self) {
        let tick = self.inner.tick.load(Ordering::Relaxed);
        self.inner
            .tick
            .store(tick.wrapping_sub(1), Ordering::Relaxed);
    }
}

impl<T: Send + Sync + 'static> Deref for Signal<T> {
    type Target = Arc<SignalInner<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Send + Sync + 'static> SignalInner<T> {
    /// Send a value, does not increment the read tick.
    pub fn send(&self, value: T) {
        let mut lock = self.inner.data.write();
        *lock = value;
        self.inner.tick.fetch_add(1, Ordering::Relaxed);
        let mut wakers = self.inner.wakers.lock();
        wakers.drain(..).for_each(|x| x.wake());
    }

    /// Send a value if changed, does not increment the read tick.
    pub fn send_if_changed(&self, value: T)
    where
        T: PartialEq,
    {
        let mut lock = self.inner.data.write();
        if *lock != value {
            *lock = value;
            let mut wakers = self.inner.wakers.lock();
            wakers.drain(..).for_each(|x| x.wake());
            self.inner.tick.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Send a value and increment the read tick.
    pub fn broadcast(&self, value: T) {
        let mut lock = self.inner.data.write();
        *lock = value;
        let version = self.inner.tick.fetch_add(1, Ordering::Relaxed);
        let mut wakers = self.inner.wakers.lock();
        wakers.drain(..).for_each(|x| x.wake());
        self.tick.store(version.wrapping_add(1), Ordering::Relaxed)
    }

    /// Send a value and increment the read tick.
    pub fn broadcast_if_changed(&self, value: T)
    where
        T: PartialEq,
    {
        let mut lock = self.inner.data.write();
        if *lock != value {
            *lock = value;
            let version = self.inner.tick.fetch_add(1, Ordering::Relaxed);
            let mut wakers = self.inner.wakers.lock();
            wakers.drain(..).for_each(|x| x.wake());
            self.tick.store(version.wrapping_add(1), Ordering::Relaxed)
        }
    }

    /// Reads the underlying value synchronously if changed.
    pub fn try_read(&self) -> Option<T>
    where
        T: Clone,
    {
        let version = self.inner.tick.load(Ordering::Relaxed);
        if self.tick.swap(version, Ordering::Relaxed) != version {
            Some(self.inner.data.read().clone())
        } else {
            None
        }
    }

    /// Reads the underlying value synchronously regardless of change detection.
    pub fn force_read(&self) -> T
    where
        T: Clone,
    {
        let version = self.inner.tick.load(Ordering::Relaxed);
        self.tick.swap(version, Ordering::Relaxed);
        self.inner.data.read().clone()
    }

    /// Poll the signal value asynchronously.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub fn poll(self: &Arc<Self>) -> SignalFuture<T>
    where
        T: Clone,
    {
        SignalFuture {
            signal: self.clone(),
            is_terminated: false,
        }
    }
}

/// Future for polling a [`Signal`] once.
pub struct SignalFuture<T: Clone> {
    signal: Arc<SignalInner<T>>,
    is_terminated: bool,
}

impl<T: Clone> Unpin for SignalFuture<T> {}

impl<T: Clone> Future for SignalFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let tick = self.signal.inner.tick.load(Ordering::Relaxed);
        if self.signal.tick.swap(tick, Ordering::Relaxed) != tick {
            self.is_terminated = true;
            Poll::Ready(self.signal.inner.data.read().clone())
        } else {
            let mut lock = self.signal.inner.wakers.lock();
            lock.push(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl<T: Clone> FusedFuture for SignalFuture<T> {
    fn is_terminated(&self) -> bool {
        self.is_terminated
    }
}

impl<T> Unpin for Signal<T> {}
impl<T> Unpin for SignalBorrow<T> {}

impl<T: Clone + Send + Sync + 'static> Stream for Signal<T> {
    type Item = T;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.0.try_read() {
            Some(result) => Poll::Ready(Some(result)),
            None => {
                let mut lock = self.0.inner.wakers.lock();
                lock.push(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Stream for SignalBorrow<T> {
    type Item = T;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.0.try_read() {
            Some(result) => Poll::Ready(Some(result)),
            None => {
                let mut lock = self.0.inner.wakers.lock();
                lock.push(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Sink<T> for Signal<T> {
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.send(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl<T: Clone + Send + Sync + 'static> Sink<T> for SignalBorrow<T> {
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.send(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
