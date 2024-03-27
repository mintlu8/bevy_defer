use std::{any::Any, marker::PhantomData, pin::pin};
use std::future::Future;
use std::sync::Arc;
use bevy_ecs::entity::Entity;
use crate::async_world::AsyncWorldMut;
use crate::async_systems::AsyncEntityParam;
use super::{signal_component::Signals, signal_inner::SignalInner};

/// A marker type that indicates the type and purpose of a signal.
pub trait SignalId: Any + Send + Sync + 'static{
    type Data: Send + Sync + Default + Clone + 'static;
}

/// Quickly construct multiple marker [`SignalId`]s at once.
/// 
/// # Example
/// ```
/// # use bevy_defer::signal_ids;
/// # pub type Vec2 = ();
/// signal_ids! {
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

/// Standard [`SignalId`] for every type.
pub enum Fac<T:  Clone + Default + Send + Sync + 'static> {
    __Phantom(PhantomData<T>, std::convert::Infallible)
}

impl<T: Clone + Default + Send + Sync + 'static> SignalId for Fac<T> {
    type Data = T;
}

/// [`AsyncEntityParam`] for sending a signal.
pub struct Sender<T: SignalId>(Arc<SignalInner<T::Data>>, PhantomData<T>);

impl<T: SignalId> Sender<T> {
    /// Send a value with a signal, can be polled by the same sender.
    pub fn send(self, item: T::Data) -> impl FnOnce() + Send + Sync + 'static  {
        move ||self.0.send(item)
    }

    /// Send a value with a signal, cannot be polled by the same sender.
    pub fn broadcast(self, item: T::Data) -> impl FnOnce() + Send + Sync + 'static  {
        move ||self.0.broadcast(item)
    }

    /// Receives a value from the sender.
    pub async fn recv(&self) -> T::Data {
        self.await
    }
}

impl<T: SignalId> AsyncEntityParam for Sender<T>  {
    type Signal = Arc<SignalInner<T::Data>>;
    
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal> {
        signals.borrow_sender::<T>()
    }

    fn from_async_context(
            _: Entity,
            _: &AsyncWorldMut,
            signal: Self::Signal,
            _: &[Entity],
        ) -> Option<Self> {
        Some(Sender(
            signal,
            PhantomData
        ))
    }
}

/// [`AsyncEntityParam`] for receiving a signal.
pub struct Receiver<T: SignalId>(Arc<SignalInner<T::Data>>, PhantomData<T>);

impl<T: SignalId> Future for &Receiver<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        pin!(signal.poll()).poll(cx)
    }
}

impl<T: SignalId> Future for &Sender<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        pin!(signal.poll()).poll(cx)
    }
}

impl<T: SignalId> Future for Receiver<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        pin!(signal.poll()).poll(cx)
    }
}

impl<T: SignalId> Future for Sender<T> {
    type Output = T::Data;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let signal = self.0.clone();
        pin!(signal.poll()).poll(cx)
    }
}

impl<T: SignalId> Receiver<T> {
    /// Receive a signal.
    pub async fn recv(&self) -> T::Data {
        self.await
    }
}

impl<T: SignalId> AsyncEntityParam for Receiver<T>  {
    type Signal = Arc<SignalInner<T::Data>>;
    
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal> {
        signals.borrow_receiver::<T>()
    }

    fn from_async_context(
            _: Entity,
            _: &AsyncWorldMut,
            signal: Self::Signal,
            _: &[Entity],
    ) -> Option<Self> {
        Some(Receiver(
            signal,
            PhantomData
        ))
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
