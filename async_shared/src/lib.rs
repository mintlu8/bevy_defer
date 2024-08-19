use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, RwLock,
    },
};

use event_listener::Event;
use futures_core::{FusedFuture, FusedStream};
use futures_util::{stream::unfold, FutureExt};

#[derive(Debug)]
pub struct Value<T> {
    inner: Arc<ValueInner<T>>,
    tick: AtomicU32,
}

impl<T: Send + Sync + 'static> Default for Value<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync + 'static> Value<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ValueInner {
                value: RwLock::new(None),
                tick: AtomicU32::new(0),
                event: Event::new(),
            }),
            tick: AtomicU32::new(0),
        }
    }

    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub fn write(&self, item: T) {
        let _ = self.inner.write(item);
    }

    pub fn write_and_tick(&self, item: T) {
        let tick = self.inner.write(item);
        self.tick.store(tick, Ordering::Release);
    }

    pub fn write_if_changed(&self, item: T)
    where
        T: PartialEq,
    {
        let _ = self.inner.write_if_changed(item);
    }

    pub fn write_if_changed_and_tick(&self, item: T)
    where
        T: PartialEq,
    {
        let tick = self.inner.write_if_changed(item);
        self.tick.store(tick, Ordering::Release);
    }

    /// Read a value if changed.
    pub fn read(&self) -> Option<T>
    where
        T: Clone,
    {
        self.inner
            .read(self.tick.load(Ordering::Acquire))
            .map(|(value, tick)| {
                self.tick.store(tick, Ordering::Release);
                value
            })
    }

    /// Read and ignore change detection, this always reads a value if initialized.
    pub fn force_read(&self) -> Option<T>
    where
        T: Clone,
    {
        self.inner.force_read().map(|(value, tick)| {
            self.tick.store(tick, Ordering::Release);
            value
        })
    }

    /// Read a value once changed asynchronously.
    pub fn read_async(&self) -> impl FusedFuture<Output = T> + Unpin + '_
    where
        T: Clone,
    {
        self.inner
            .read_async(self.tick.load(Ordering::Acquire))
            .inspect(|(_, tick)| self.tick.store(*tick, Ordering::Release))
            .map(|(v, _)| v)
    }

    /// Read a value once changed asynchronously.
    pub fn read_async_owned(self) -> impl FusedFuture<Output = (T, Self)> + Unpin + 'static
    where
        T: Clone,
    {
        self.inner
            .clone()
            .read_async_arc(self.tick.load(Ordering::Acquire))
            .map(|(v, tick)| {
                self.tick.store(tick, Ordering::Release);
                (v, self)
            })
    }

    /// Read a value once changed asynchronously.
    pub fn read_async_arc(self: Arc<Self>) -> impl FusedFuture<Output = T> + Unpin + 'static
    where
        T: Clone,
    {
        self.inner
            .clone()
            .read_async_arc(self.tick.load(Ordering::Acquire))
            .inspect(move |(_, tick)| self.tick.store(*tick, Ordering::Release))
            .map(|(v, _)| v)
    }

    /// Rewind the tick to make the underlying value readable.
    pub fn make_readable(&self) {
        self.tick
            .store(self.inner.read_tick().wrapping_sub(1), Ordering::Release)
    }

    /// Clone and duplicate the read tick, this makes the current value unreadable.
    pub fn clone_uninit(&self) -> Self {
        Value {
            inner: self.inner.clone(),
            tick: AtomicU32::new(self.inner.read_tick()),
        }
    }

    /// Clone and decrement the read tick, this makes the current value always readable.
    pub fn clone_init(&self) -> Self {
        Value {
            inner: self.inner.clone(),
            tick: AtomicU32::new(self.inner.read_tick().wrapping_sub(1)),
        }
    }

    /// The real `Clone` implementation that does not guarantee value is readable.
    ///
    /// `Clone` is not implemented since we primarily want to use `Arc::clone` with this type.
    pub fn clone_raw(&self) -> Self {
        Value {
            inner: self.inner.clone(),
            tick: AtomicU32::new(self.tick.load(Ordering::Acquire)),
        }
    }

    /// Convert into `Arc<Value<T>>`.
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    pub fn to_stream(&self) -> impl FusedStream<Item = T> + Unpin + '_
    where
        T: Clone,
    {
        unfold(self, |value| {
            value.read_async().map(move |x| Some((x, value)))
        })
    }

    pub fn into_stream(self) -> impl FusedStream<Item = T> + Unpin
    where
        T: Clone,
    {
        unfold(self, |value| value.read_async_owned().map(Some))
    }

    pub fn into_stream_arc(self: Arc<Self>) -> impl FusedStream<Item = T> + Unpin
    where
        T: Clone,
    {
        unfold(self, move |value| {
            let fut = value.clone().read_async_arc();
            fut.map(move |x| Some((x, value)))
        })
    }
}

#[derive(Debug)]
struct ValueInner<T> {
    value: RwLock<Option<T>>,
    tick: AtomicU32,
    event: Event,
}

impl<T: Send + Sync> ValueInner<T> {
    fn read_tick(&self) -> u32 {
        self.tick.load(Ordering::Relaxed)
    }

    fn write(&self, item: T) -> u32 {
        let result = self.tick.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        *self.value.write().unwrap() = Some(item);
        self.event.notify(usize::MAX);
        result
    }

    fn write_if_changed(&self, item: T) -> u32
    where
        T: PartialEq,
    {
        let mut lock = self.value.write().unwrap();
        if lock.as_ref().is_some_and(|x| x == &item) {
            let result = self.tick.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
            *lock = Some(item);
            self.event.notify(usize::MAX);
            result
        } else {
            self.read_tick()
        }
    }

    fn read_async<'t>(&'t self, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)> + 't>>
    where
        T: Clone,
    {
        Box::pin(async move {
            loop {
                let new_tick = self.tick.load(Ordering::Acquire);
                if new_tick != tick {
                    if let Some(result) = self.value.read().unwrap().clone() {
                        return (result, new_tick);
                    }
                } else {
                    let _ = self.event.listen().await;
                }
            }
        })
    }

    fn read_async_arc(self: Arc<Self>, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)>>>
    where
        T: Clone + 'static,
    {
        Box::pin(async move {
            loop {
                let new_tick = self.tick.load(Ordering::Acquire);
                if new_tick != tick {
                    if let Some(result) = self.value.read().unwrap().clone() {
                        return (result, new_tick);
                    }
                } else {
                    let _ = self.event.listen().await;
                }
            }
        })
    }

    fn read(&self, tick: u32) -> Option<(T, u32)>
    where
        T: Clone,
    {
        let new_tick = self.tick.load(Ordering::Acquire);
        if self.tick.load(Ordering::Acquire) != tick {
            self.value.read().unwrap().clone().map(|x| (x, new_tick))
        } else {
            None
        }
    }

    fn force_read(&self) -> Option<(T, u32)>
    where
        T: Clone,
    {
        self.value
            .read()
            .unwrap()
            .clone()
            .map(|x| (x, self.tick.load(Ordering::Acquire)))
    }
}
