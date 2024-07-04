use std::{
    error::Error,
    fmt::{Debug, Display},
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, RwLock,
    },
    task::Poll,
};

use adaptors::{MappedReadonlyValue, MappedRwValue};
use event_listener::Event;
use futures_core::{FusedStream, Stream};
pub mod adaptors;

#[derive(Debug)]
pub struct Value<T> {
    inner: Arc<dyn ValueInner<T>>,
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
            inner: Arc::new(RawValue {
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

    pub fn write(&self, item: T) -> Result<(), ValueIsReadOnly> {
        let _ = self.inner.write(item)?;
        Ok(())
    }

    pub fn write_and_tick(&self, item: T) -> Result<(), ValueIsReadOnly> {
        let tick = self.inner.write(item)?;
        self.tick.store(tick, Ordering::Release);
        Ok(())
    }

    pub fn write_if_changed(&self, item: T) -> Result<(), ValueIsReadOnly>
    where
        T: PartialEq,
    {
        let _ = self.inner.write_if_changed(item)?;
        Ok(())
    }

    pub fn write_if_changed_and_tick(&self, item: T) -> Result<(), ValueIsReadOnly>
    where
        T: PartialEq,
    {
        let tick = self.inner.write_if_changed(item)?;
        self.tick.store(tick, Ordering::Release);
        Ok(())
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
    pub async fn read_async(&self) -> T
    where
        T: Clone,
    {
        let (value, tick) = self
            .inner
            .read_async(self.tick.load(Ordering::Acquire))
            .await;
        self.tick.store(tick, Ordering::Release);
        value
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

    /// The real `Clone` implementation.
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

    pub fn to_stream(&self) -> ValueStream<T>
    where
        T: Clone,
    {
        ValueStream {
            value: OwnedOrRef::Ref(self),
            listener: None,
        }
    }

    pub fn into_stream(self) -> ValueStream<'static, T>
    where
        T: Clone,
    {
        ValueStream {
            value: OwnedOrRef::Owned(self),
            listener: None,
        }
    }

    pub fn into_stream_arc(self: Arc<Self>) -> ValueStream<'static, T>
    where
        T: Clone,
    {
        ValueStream {
            value: OwnedOrRef::Arc(self),
            listener: None,
        }
    }

    pub fn map_read_only<U>(&self, f: impl Fn(T) -> U + Send + Sync + 'static) -> Value<U>
    where
        T: Clone,
        U: Clone + Send + Sync + 'static,
    {
        Value {
            inner: Arc::new(MappedReadonlyValue {
                value: self.inner.clone(),
                mapper: f,
                p: PhantomData,
            }),
            tick: AtomicU32::new(self.tick.load(Ordering::Acquire)),
        }
    }

    pub fn map_rw<U>(
        &self,
        f1: impl Fn(T) -> U + Send + Sync + 'static,
        f2: impl Fn(U) -> T + Send + Sync + 'static,
    ) -> Value<U>
    where
        T: Clone,
        U: Clone + Send + Sync + 'static,
    {
        Value {
            inner: Arc::new(MappedRwValue {
                value: self.inner.clone(),
                w2r: f1,
                r2w: f2,
                p: PhantomData,
            }),
            tick: AtomicU32::new(self.tick.load(Ordering::Acquire)),
        }
    }
}

/// [`Error`] that [`Value`] is read only.
#[derive(Debug)]
pub struct ValueIsReadOnly;

impl Display for ValueIsReadOnly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Value is read only.")
    }
}

impl Error for ValueIsReadOnly {}

pub trait ValueInner<T>: Send + Sync {
    fn as_debug(&self) -> &dyn Debug
    where
        T: Debug,
    {
        #[derive(Debug)]
        pub struct UnknownValue;
        &UnknownValue
    }
    fn read_tick(&self) -> u32;
    fn write(&self, item: T) -> Result<u32, ValueIsReadOnly>;
    fn write_if_changed(&self, item: T) -> Result<u32, ValueIsReadOnly>
    where
        T: PartialEq;
    fn read_async<'t>(&'t self, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)> + 't>>
    where
        T: Clone;
    fn read(&self, tick: u32) -> Option<(T, u32)>
    where
        T: Clone;
    fn force_read(&self) -> Option<(T, u32)>
    where
        T: Clone;
}

impl<T: Debug> Debug for dyn ValueInner<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_debug().fmt(f)
    }
}

#[derive(Debug)]
struct RawValue<T> {
    value: RwLock<Option<T>>,
    tick: AtomicU32,
    event: Event,
}

impl<T> ValueInner<T> for Arc<dyn ValueInner<T>> {
    fn read_tick(&self) -> u32 {
        Arc::as_ref(self).read_tick()
    }

    fn write(&self, item: T) -> Result<u32, ValueIsReadOnly> {
        Arc::as_ref(self).write(item)
    }

    fn write_if_changed(&self, item: T) -> Result<u32, ValueIsReadOnly>
    where
        T: PartialEq,
    {
        Arc::as_ref(self).write_if_changed(item)
    }

    fn read_async<'t>(&'t self, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)> + 't>>
    where
        T: Clone,
    {
        Arc::as_ref(self).read_async(tick)
    }

    fn read(&self, tick: u32) -> Option<(T, u32)>
    where
        T: Clone,
    {
        Arc::as_ref(self).read(tick)
    }

    fn force_read(&self) -> Option<(T, u32)>
    where
        T: Clone,
    {
        Arc::as_ref(self).force_read()
    }
}

impl<T, A: ValueInner<T>> ValueInner<T> for Arc<A> {
    fn read_tick(&self) -> u32 {
        Arc::as_ref(self).read_tick()
    }

    fn write(&self, item: T) -> Result<u32, ValueIsReadOnly> {
        Arc::as_ref(self).write(item)
    }

    fn write_if_changed(&self, item: T) -> Result<u32, ValueIsReadOnly>
    where
        T: PartialEq,
    {
        Arc::as_ref(self).write_if_changed(item)
    }

    fn read_async<'t>(&'t self, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)> + 't>>
    where
        T: Clone,
    {
        Arc::as_ref(self).read_async(tick)
    }

    fn read(&self, tick: u32) -> Option<(T, u32)>
    where
        T: Clone,
    {
        Arc::as_ref(self).read(tick)
    }

    fn force_read(&self) -> Option<(T, u32)>
    where
        T: Clone,
    {
        Arc::as_ref(self).force_read()
    }
}

impl<T: Send + Sync> ValueInner<T> for RawValue<T> {
    fn as_debug(&self) -> &dyn Debug
    where
        T: Debug,
    {
        self
    }

    fn read_tick(&self) -> u32 {
        self.tick.load(Ordering::Relaxed)
    }

    fn write(&self, item: T) -> Result<u32, ValueIsReadOnly> {
        let result = self.tick.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        *self.value.write().unwrap() = Some(item);
        self.event.notify(usize::MAX);
        Ok(result)
    }

    fn write_if_changed(&self, item: T) -> Result<u32, ValueIsReadOnly>
    where
        T: PartialEq,
    {
        let mut lock = self.value.write().unwrap();
        if lock.as_ref().is_some_and(|x| x == &item) {
            let result = self.tick.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
            *lock = Some(item);
            self.event.notify(usize::MAX);
            Ok(result)
        } else {
            Ok(self.read_tick())
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

enum OwnedOrRef<'t, T> {
    Owned(T),
    Arc(Arc<T>),
    Ref(&'t T),
}

impl<'t, T> OwnedOrRef<'t, T> {
    /// Safety: self must be valid for the duration of `'t`.
    unsafe fn as_ref(&self) -> &'t T {
        match self {
            OwnedOrRef::Owned(item) => unsafe { (item as *const T).as_ref().unwrap() },
            OwnedOrRef::Arc(item) => unsafe { (item.as_ref() as *const T).as_ref().unwrap() },
            OwnedOrRef::Ref(item) => item,
        }
    }
}
/// A [`Stream`] that polls values continuously from a signal.
pub struct ValueStream<'t, T: Clone> {
    value: OwnedOrRef<'t, Value<T>>,
    listener: Option<Pin<Box<dyn Future<Output = T> + 't>>>,
}

impl<T: Clone + Send + Sync + 'static> Stream for ValueStream<'_, T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if let Some(fut) = &mut self.listener {
            fut.as_mut().poll(cx).map(|x| {
                self.listener = None;
                Some(x)
            })
        } else {
            // Safety: Safe since self.value is valid for the duration of the stream and T is static.
            let mut fut = Box::pin(unsafe { self.value.as_ref() }.read_async());
            match fut.as_mut().poll(cx) {
                Poll::Ready(result) => return Poll::Ready(Some(result)),
                Poll::Pending => (),
            }
            self.listener = Some(fut as Pin<Box<dyn Future<Output = T>>>);
            Poll::Pending
        }
    }
}

impl<T: Clone + Send + Sync + 'static> FusedStream for ValueStream<'_, T> {
    fn is_terminated(&self) -> bool {
        false
    }
}
