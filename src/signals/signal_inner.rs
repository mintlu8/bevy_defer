use std::convert::Infallible;
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::{ops::Deref, sync::atomic::AtomicU32, task::Poll};

use event_listener::{Event, EventListener};
use event_listener_strategy::{EventListenerFuture, FutureWrapper, NonBlocking, Strategy};
use futures::future::{Fuse, FusedFuture};
use futures::stream::FusedStream;
use futures::{Future, FutureExt, Sink, Stream};
use parking_lot::RwLock;
use std::sync::atomic::Ordering;

/// The data component of a signal.
#[derive(Debug, Default)]
pub(crate) struct SignalData<T> {
    pub(crate) data: RwLock<T>,
    pub(crate) tick: AtomicU32,
    pub(crate) event: Event,
}

/// The shared component of a signal.
#[derive(Debug, Default)]
pub struct SignalInner<T> {
    pub(crate) inner: Arc<SignalData<T>>,
    pub(crate) tick: AtomicU32,
}

/// A piece of shared data that can be read once per write.
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct Signal<T>(Arc<SignalInner<T>>);

/// A borrowed signal that shares its read tick.
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct SignalBorrow<T>(Signal<T>);

impl<T> Deref for Signal<T> {
    type Target = Arc<SignalInner<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for SignalBorrow<T> {
    type Target = Signal<T>;

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

impl<T> Clone for SignalBorrow<T> {
    fn clone(&self) -> Self {
        SignalBorrow(Signal(self.0.0.clone()))
    }
}

impl<T: Send + Sync + 'static> Signal<T> {
    /// Create a new signal.
    pub fn new(value: T) -> Self {
        Self(Arc::new(SignalInner {
            inner: Arc::new(SignalData {
                data: RwLock::new(value),
                tick: AtomicU32::new(0),
                event: Event::new(),
            }),
            tick: AtomicU32::new(0),
        }))
    }

    /// Borrow the inner value with shared read tick.
    pub fn borrow_inner(&self) -> SignalBorrow<T> {
        SignalBorrow(Signal(self.0.clone()))
    }

    /// Rewind the read tick and allow the current value to be read.
    pub fn rewind(&self) {
        let tick = self.inner.tick.load(Ordering::Relaxed);
        self.inner
            .tick
            .store(tick.wrapping_sub(1), Ordering::Relaxed);
    }
}

impl<T: Clone + Send + Sync + 'static> Signal<T> {
    pub fn into_stream(self) -> SignalStream<T> {
        SignalStream {
            signal: self,
            listener: None,
        }
    }
}

impl<T: Clone + Send + Sync + 'static> SignalBorrow<T> {
    pub fn into_stream(self) -> SignalStream<T> {
        SignalStream {
            signal: self.0,
            listener: None,
        }
    }
}

impl<T: Send + Sync + 'static> Signal<T> {
    /// Send a value, does not increment the read tick.
    pub fn send(&self, value: T) {
        let mut lock = self.inner.data.write();
        *lock = value;
        self.inner.tick.fetch_add(1, Ordering::Relaxed);
        self.inner.event.notify(usize::MAX);
    }

    /// Send a value if changed, does not increment the read tick.
    pub fn send_if_changed(&self, value: T)
    where
        T: PartialEq,
    {
        let mut lock = self.inner.data.write();
        if *lock != value {
            *lock = value;
            self.inner.tick.fetch_add(1, Ordering::Relaxed);
            self.inner.event.notify(usize::MAX);
        }
    }

    /// Send a value and increment the read tick.
    pub fn broadcast(&self, value: T) {
        let mut lock = self.inner.data.write();
        *lock = value;
        let version = self.inner.tick.fetch_add(1, Ordering::Relaxed);        
        self.inner.event.notify(usize::MAX);
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
            self.inner.event.notify(usize::MAX);
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
    pub fn poll(&self) -> SignalFuture<T>
    where
        T: Clone,
    {
        SignalFuture(FutureWrapper::new(SignalFutureInner {
            signal: self.0.clone(),
            listener: Some(self.inner.event.listen()),
        }).fuse())
    }
}

/// A [`FusedFuture`] that polls a single value from a signal.
pub struct SignalFuture<T: Clone>(
    Fuse<FutureWrapper<SignalFutureInner<T>>>
);

impl<T: Clone> Unpin for SignalFuture<T> {}

impl<T: Clone> Future for SignalFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

impl<T: Clone> FusedFuture for SignalFuture<T> {
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

/// Future for polling a [`Signal`] once.
pub struct SignalFutureInner<T: Clone> {
    signal: Arc<SignalInner<T>>,
    listener: Option<EventListener>,
}

impl<T: Clone> Unpin for SignalFutureInner<T> {}

impl<T: Clone> EventListenerFuture for SignalFutureInner<T> {
    type Output = T;

    fn poll_with_strategy<'a, S: event_listener_strategy::Strategy<'a>>(
        mut self: Pin<&mut Self>,
        strategy: &mut S,
        cx: &mut S::Context,
    ) -> Poll<Self::Output> {
        let tick = self.signal.inner.tick.load(Ordering::Relaxed);
        loop {
            if self.signal.tick.swap(tick, Ordering::Relaxed) != tick {
                return Poll::Ready(self.signal.inner.data.read().clone())
            } else {
                match strategy.poll(&mut self.listener, cx) {
                    Poll::Ready(_) => (),
                    Poll::Pending => return Poll::Pending,
                };
            }
        }
    }
}

impl<T> Unpin for Signal<T> {}
impl<T> Unpin for SignalBorrow<T> {}


/// A [`Stream`] that polls values continuously from a signal.
pub struct SignalStream<T: Clone> {
    signal: Signal<T>,
    listener: Option<EventListener>,
}

impl<T: Clone> Unpin for SignalStream<T> {}

impl<T: Clone + Send + Sync + 'static> Stream for SignalStream<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            match self.signal.try_read() {
                Some(result) => return Poll::Ready(Some(result)),
                None => {
                    if self.listener.is_none() {
                        self.listener = Some(self.signal.inner.event.listen());
                    }
                    match NonBlocking::default().poll(&mut self.listener, cx) {
                        Poll::Ready(()) => (),
                        Poll::Pending => return Poll::Pending,
                    }
                }
            }
        }
    }
}

impl<T: Clone + Send + Sync + 'static> FusedStream for SignalStream<T> {
    fn is_terminated(&self) -> bool {
        false
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
