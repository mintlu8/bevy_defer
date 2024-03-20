use std::{any::{Any, TypeId}, fmt::Debug, marker::PhantomData, pin::pin, rc::Rc, sync::atomic::Ordering, task::Poll};
use std::future::Future;
use triomphe::Arc;
use bevy_ecs::entity::Entity;
use crate::object::{Object, AsObject};
use crate::{AsyncQueryQueue, AsyncEntityParam};
use super::{component::Signals, signal_inner::SignalInner};
pub use super::signal_inner::{Signal, SignalData};

/// A marker type that indicates the type and purpose of a signal.
pub trait SignalId: Any + Send + Sync + 'static{
    type Data: AsObject + Default;
}

/// Quickly construct multiple marker [`SignalId`]s at once.
/// 
/// # Example
/// ```
/// signal_ids!{
///     /// Shared factor as a f32
///     SharedFactor: f32,
///     /// Shared position as a Vec2
///     pub SharedPosition: Vec2,
/// }
/// ```
#[macro_export]
macro_rules! signal_ids {
    ($($(#[$($attr:tt)*])*$vis: vis $name: ident: $ty: ty),* $(,)?) => {
        $(
            $(#[$($attr)*])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            $vis enum $name {}

            impl $crate::signals::SignalId for $name{
                type Data = $ty;
            }
        )*
    };
}


/// A type erased signal with a nominal type.
#[derive(Debug, Clone)]
pub struct TypedSignal<T: AsObject> {
    pub(crate) inner: Arc<SignalData<Object>>,
    p: PhantomData<T>,
}

impl<T: AsObject> Default for TypedSignal<T> {
    fn default() -> Self {
        Self { inner: Default::default(), p: PhantomData }
    }
}

impl<T: AsObject> TypedSignal<T> {

    pub fn new() -> Self {
        Self { inner: Default::default(), p: PhantomData }
    }

    pub fn from_inner(inner: Arc<SignalData<Object>>) -> Self {
        Self {
            inner,
            p: PhantomData
        }
    }

    pub fn from_signal(signal: &Signal<Object>) -> Self {
        Self {
            inner: signal.inner.inner.clone(),
            p: PhantomData
        }
    }
    
    pub fn into_inner(self) -> Arc<SignalData<Object>> {
        self.inner
    }

    pub fn type_erase(self) -> TypedSignal<Object> {
        TypedSignal { 
            inner: self.inner, 
            p: PhantomData 
        }
    }

    pub fn send(&self, item: T) {
        let mut lock = self.inner.data.lock();
        lock.set(item);
        self.inner.tick.fetch_add(1, Ordering::Relaxed);
        let mut wakers = self.inner.wakers.lock();
        wakers.drain(..).for_each(|x| x.wake());
    }

    pub fn peek(&self) -> Option<T>{
        let lock = self.inner.data.lock();
        lock.get()
    }
}

impl TypedSignal<Object> {
    pub fn of_type<T: AsObject>(self) -> TypedSignal<T> {
        TypedSignal { 
            inner: self.inner, 
            p: PhantomData 
        }
    }
}

pub(crate) trait SignalMapperTrait: Send + Sync + 'static {
    fn map(&self, obj: &mut Object);
    fn dyn_clone(&self) -> Box<dyn SignalMapperTrait>;
}

impl<T> SignalMapperTrait for T where T: Fn(&mut Object) + Clone + Send + Sync + 'static {
    fn map(&self, obj: &mut Object) {
        self(obj)
    }
    fn dyn_clone(&self) -> Box<dyn SignalMapperTrait> {
        Box::new(self.clone())
    }
}

/// A function that maps a signal's value.
pub struct SignalMapper(Box<dyn SignalMapperTrait>);

impl Debug for SignalMapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalMapper").finish()
    }
}

impl Clone for SignalMapper {
    fn clone(&self) -> Self {
        Self(self.0.dyn_clone())
    }
}

impl SignalMapper {
    pub fn new<A: SignalId, B: SignalId>(f: impl Fn(A::Data) -> B::Data + Clone + Send + Sync + 'static) -> Self {
        Self(Box::new(move |obj: &mut Object| {
            let Some(item) = obj.clone().get::<A::Data>() else {return};
            *obj = Object::new(f(item));
        }))
    }

    pub fn map<T: AsObject>(&self, mut obj: Object) -> Option<T> {
        self.0.map(&mut obj);
        obj.get()
    }
}

/// `AsyncSystemParam` for sending a signal.
pub struct Sender<T: SignalId>(Arc<SignalInner<Object>>, PhantomData<T>);

impl<T: SignalId> Sender<T> {
    /// Send a value with a signal, can be polled by the same sender.
    pub fn send(self, item: T::Data) -> impl Fn() + Send + Sync + 'static  {
        let obj = Object::new(item);
        move ||self.0.write(obj.clone())
    }

    /// Send a value with a signal, cannot be polled by the same sender.
    pub fn broadcast(self, item: T::Data) -> impl Fn() + Send + Sync + 'static  {
        let obj = Object::new(item);
        move ||self.0.broadcast(obj.clone())
    }

    /// Receives a value from the sender.
    pub async fn recv(&self) -> T::Data {
        self.await
    }
}

impl <'t, T: SignalId> AsyncEntityParam<'t> for Sender<T>  {
    type Signal = Arc<SignalInner<Object>>;
    
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal> {
        signals.borrow_sender::<T>()
    }

    fn from_async_context(
            _: Entity,
            _: &Rc<AsyncQueryQueue>,
            signal: Self::Signal,
        ) -> Self {
        Sender(
            signal,
            PhantomData
        )
    }
}

/// `AsyncSystemParam` for receiving a signal.
pub struct Receiver<T: SignalId>(Arc<SignalInner<Object>>, PhantomData<T>);

impl<T: SignalId> Future for &Receiver<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        let pinned = pin!(signal.async_read());
        match pinned.poll(cx) {
            Poll::Ready(data) => if let Some(data) = data.get() {
                Poll::Ready(data)
            } else {
                Poll::Pending
            }
            Poll::Pending => Poll::Pending
        }
    }
}

impl<T: SignalId> Future for &Sender<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        let pinned = pin!(signal.async_read());
        match pinned.poll(cx) {
            Poll::Ready(data) => if let Some(data) = data.get() {
                Poll::Ready(data)
            } else {
                Poll::Pending
            }
            Poll::Pending => Poll::Pending
        }
    }
}

impl<T: SignalId> Future for Receiver<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        let pinned = pin!(signal.async_read());
        match pinned.poll(cx) {
            Poll::Ready(data) => if let Some(data) = data.get() {
                Poll::Ready(data)
            } else {
                Poll::Pending
            }
            Poll::Pending => Poll::Pending
        }
    }
}

impl<T: SignalId> Future for Sender<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        let pinned = pin!(signal.async_read());
        match pinned.poll(cx) {
            Poll::Ready(data) => if let Some(data) = data.get() {
                Poll::Ready(data)
            } else {
                Poll::Pending
            }
            Poll::Pending => Poll::Pending
        }
    }
}

impl<T: SignalId> Receiver<T> {
    /// Receive a signal.
    pub async fn recv(&self) -> T::Data {
        self.await
    }
}

impl<T: SignalId<Data = Object>> Receiver<T> {
    /// Receives and downcasts a signal, discard all invalid typed values.
    pub async fn recv_as<A: AsObject>(&self) -> A {
        loop {
            let signal = self.0.clone();
            let obj = signal.async_read().await;
            if let Some(data) = obj.get() {
                return data;
            }
        }
    }
}


impl <'t, T: SignalId> AsyncEntityParam<'t> for Receiver<T>  {
    type Signal = Arc<SignalInner<Object>>;
    
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal> {
        signals.borrow_receiver::<T>()
    }

    fn from_async_context(
            _: Entity,
            _: &Rc<AsyncQueryQueue>,
            signal: Self::Signal,
        ) -> Self {
        Receiver(
            signal,
            PhantomData
        )
    }
}

mod sealed {
    use std::marker::PhantomData;

    use bevy_ecs::query::QueryData;

    use super::{SignalId, Signals};

    /// `WorldQuery` for sending a signal synchronously.
    /// 
    /// This does not filter for [`Signals`] or require mutable access.
    #[derive(Debug, QueryData)]
    pub struct SignalSender<T: SignalId>{
        signals: Option<&'static Signals>,
        p: PhantomData<T>,
    }

    impl<T: SignalId> SignalSenderItem<'_, T> {
        /// Check if a sender exists.
        pub fn exists(&self) -> bool{
            self.signals
                .map(|x| x.borrow_sender::<T>().is_some())
                .unwrap_or(false)
        }

        /// Send a item through a signal, can be polled from the same sender.
        pub fn send(&self, item: T::Data) {
            if let Some(signals) = self.signals {
                signals.send::<T>(item);
            }
        }
        
        /// Send a item through a signal, cannot be polled from the same sender.
        pub fn broadcast(&self, item: T::Data) {
            if let Some(signals) = self.signals {
                signals.broadcast::<T>(item);
            }
        }

        /// Poll the signal from a sender.
        pub fn poll_sender(&self) -> Option<T::Data> {
            self.signals.and_then(|s| s.poll_sender_once::<T>())
        }
    }

    /// `WorldQuery` for receiving a signal synchronously.
    /// 
    /// This does not filter for [`Signals`] or require mutable access.
    #[derive(Debug, QueryData)]
    pub struct SignalReceiver<T: SignalId>{
        signals: Option<&'static Signals>,
        p: PhantomData<T>,
    }

    impl<T: SignalId> SignalReceiverItem<'_, T> {
        /// Poll an item synchronously.
        pub fn poll_once(&self) -> Option<T::Data> {
            self.signals.as_ref()
                .and_then(|sig| sig.poll_once::<T>())
        }

        /// Returns true if content is changed.
        pub fn poll_change(&self) -> bool {
            self.signals.as_ref()
                .and_then(|sig| sig.poll_once::<T>())
                .is_some()
        }
    }
}

pub use sealed::{SignalSender, SignalReceiver};

/// A signal with a role, that can be composed with [`Signals`].
pub enum RoleSignal<T: SignalId>{
    Sender(TypedSignal<T::Data>),
    Receiver(TypedSignal<T::Data>),
    Adaptor(TypeId, SignalMapper),
}

impl<T: SignalId> RoleSignal<T> {
    pub fn and<A: SignalId>(self, other: RoleSignal<A>) -> Signals {
        let base = match self {
            RoleSignal::Sender(s) => Signals::from_sender::<T>(s),
            RoleSignal::Receiver(r) => Signals::from_receiver::<T>(r),
            RoleSignal::Adaptor(t, a) => {
                let mut s = Signals::new();
                s.add_adaptor::<T>(t, a);
                s
            },
        };
        base.and(other)
    }

    pub fn into_signals(self) -> Signals {
        match self {
            RoleSignal::Sender(s) => Signals::from_sender::<T>(s),
            RoleSignal::Receiver(r) => Signals::from_receiver::<T>(r),
            RoleSignal::Adaptor(t, a) => {
                let mut s = Signals::new();
                s.add_adaptor::<T>(t, a);
                s
            },
        }
    }
}

impl Signals {
    pub fn and<A: SignalId>(self, other: RoleSignal<A>) -> Signals {
        match other {
            RoleSignal::Sender(s) => self.with_sender::<A>(s),
            RoleSignal::Receiver(r) => self.with_receiver::<A>(r),
            RoleSignal::Adaptor(t, a) => self.with_adaptor::<A>(t, a),
        }
    }

    pub fn into_signals(self) -> Signals {
        self
    }
}
