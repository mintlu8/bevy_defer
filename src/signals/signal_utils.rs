use std::future::IntoFuture;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{any::Any, marker::PhantomData};
use std::sync::Arc;
use bevy_ecs::entity::Entity;
use futures::Stream;
use crate::access::AsyncWorldMut;
use crate::async_systems::AsyncEntityParam;
use super::signal_inner::SignalFuture;
use super::Signal;
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
pub struct Fac<T:  Clone + Default + Send + Sync + 'static>(PhantomData<T>, std::convert::Infallible);

impl<T: Clone + Default + Send + Sync + 'static> SignalId for Fac<T> {
    type Data = T;
}

/// [`AsyncEntityParam`] for sending a signal.
pub struct Sender<T: SignalId>(Signal<T::Data>);

impl<T: SignalId> Unpin for Sender<T> {}

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
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub fn recv(&self) -> SignalFuture<T::Data> {
        self.0.poll()
    }
}

/// [`AsyncEntityParam`] for receiving a signal.
pub struct Receiver<T: SignalId>(Signal<T::Data>);

impl<T: SignalId> Unpin for Receiver<T> {}

impl<T: SignalId> Receiver<T> {
    /// Receive a value from the receiver.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub fn recv(&self) -> SignalFuture<T::Data> {
        self.0.poll()
    }

    /// Convert into a stream.
    pub fn into_stream(self) -> impl Stream<Item = T::Data> {
        self.0
    }
}

impl<T: SignalId> Stream for Receiver<T> {
    type Item = T::Data;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let signal = Pin::new(&mut self.0);
        Signal::poll_next(signal, cx)
    }
}

impl<T: SignalId> Stream for Sender<T> {
    type Item = T::Data;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let signal = Pin::new(&mut self.0);
        Signal::poll_next(signal, cx)
    }
}

impl<T: SignalId> IntoFuture for &Receiver<T> {
    type Output = T::Data;
    
    type IntoFuture = SignalFuture<T::Data>;
    
    fn into_future(self) -> Self::IntoFuture {
        self.0.poll()
    }
}

impl<T: SignalId> IntoFuture for &Sender<T> {
    type Output = T::Data;
    
    type IntoFuture = SignalFuture<T::Data>;
    
    fn into_future(self) -> Self::IntoFuture {
        self.0.poll()
    }
}

impl<T: SignalId> IntoFuture for Receiver<T> {
    type Output = T::Data;
    
    type IntoFuture = SignalFuture<T::Data>;
    
    fn into_future(self) -> Self::IntoFuture {
        self.0.poll()
    }
}

impl<T: SignalId> IntoFuture for Sender<T> {
    type Output = T::Data;
    
    type IntoFuture = SignalFuture<T::Data>;
    
    fn into_future(self) -> Self::IntoFuture {
        self.0.poll()
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
            Signal::from_inner(signal)
        ))
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
            Signal::from_inner(signal),
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
        /// 
        /// Returns `true` if the sender exists.
        pub fn send(&self, item: T::Data) -> bool {
            if let Some(signals) = self.signals {
                signals.send::<T>(item)
            } else {
                false
            }
        }

        /// Send a item through a signal, can be polled from the same sender.
        /// 
        /// Returns `true` if the sender exists.
        pub fn send_if_changed(&self, item: T::Data) -> bool where T::Data: PartialEq{
            if let Some(signals) = self.signals {
                signals.send_if_changed::<T>(item)
            } else {
                false
            }
        }
        
        /// Send a item through a signal, cannot be polled from the same sender.
        /// 
        /// Returns `true` if the sender exists.
        pub fn broadcast(&self, item: T::Data) -> bool{
            if let Some(signals) = self.signals {
                signals.broadcast::<T>(item)
            } else {
                false
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
