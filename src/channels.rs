//! `!Send` version of `futures_channels::oneshot`
use std::{cell::{Cell, RefCell}, pin::Pin, rc::Rc, task::{Context, Poll, Waker}};
use std::future::Future;
use futures::future::{Either, FusedFuture, Ready};

use crate::{AsyncResult, CHANNEL_CLOSED};

/// Sender for a `!Send` oneshot channel.
#[derive(Debug)]
pub struct Sender<T>(Rc<Inner<T>>);

impl<T> Unpin for Sender<T> {}

/// Receiver for a `!Send` oneshot channel.
#[derive(Debug)]
pub struct Receiver<T>(Rc<Inner<T>>);

impl<T> Unpin for Receiver<T> {}

#[derive(Debug)]
struct Lock<T>(RefCell<Option<T>>);

impl<T> Lock<T> {
    fn new() -> Self {
        Lock(RefCell::new(None))
    }

    fn set(&self, value: T) -> Result<(), T>{
        match &mut *self.0.borrow_mut() {
            Some(_) => Err(value),
            r => {
                *r = Some(value);
                Ok(())
            },
        }
    }

    fn init(&self, value: impl FnOnce() -> T) {
        match &mut *self.0.borrow_mut() {
            Some(_) => (),
            r => {
                *r = Some(value());
            },
        }
    }

    fn take(&self) -> Option<T> {
        self.0.borrow_mut().take()
    }
}

#[derive(Debug)]
struct Inner<T> {
    complete: Cell<bool>,
    data: Lock<T>,
    cancel_waker: Lock<Waker>,
    recv_waker: Lock<Waker>,
}

impl<T> Inner<T> {
    pub fn new() -> Self {
        Inner {
            complete: Cell::new(false),
            data: Lock::new(),
            cancel_waker: Lock::new(),
            recv_waker: Lock::new(),
        }
    }
}

/// Creates a new one-shot channel for sending a single value across single threaded asynchronous tasks.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Rc::new(Inner::new());
    let receiver = Receiver(inner.clone());
    let sender = Sender(inner);
    (sender, receiver)
}

impl<T> Inner<T> {
    fn send(&self, t: T) -> Result<(), T> {
        if self.complete.replace(true) {
            return Err(t);
        }
        if let Some(waker) = self.recv_waker.take() {
            waker.wake()
        }
        self.data.set(t)
    }

    fn poll_canceled(&self, cx: &mut Context<'_>) -> Poll<()> {
        if self.complete.get() {
            return Poll::Ready(());
        }
        let handle = cx.waker().clone();
        let _ = self.cancel_waker.set(handle);
        Poll::Pending
    }

    fn recv(&self, cx: &mut Context<'_>) -> Poll<Result<T, Canceled>> {
        if self.complete.get() {
            if let Some(value) = self.data.take() {
                Poll::Ready(Ok(value))
            } else {
                Poll::Ready(Err(Canceled))
            }
        } else {
            self.recv_waker.init(|| cx.waker().clone());
            Poll::Pending
        }
    }
}

impl<T> Sender<T> {
    pub fn send(self, t: T) -> Result<(), T> {
        self.0.send(t)
    }

    pub fn is_closed(&self) -> bool {
        self.0.complete.get()
    }

    pub fn cancellation(&mut self) -> ChannelCancel<T> {
        ChannelCancel(self)
    }

    pub fn by_ref(self) -> RefSender<T>{
        RefSender(Some(self))
    }
}

/// Sender for a `!Send` oneshot channel.
#[derive(Debug)]
pub struct RefSender<T>(Option<Sender<T>>);


impl<T> RefSender<T> {
    pub fn send(&mut self, t: T) {
        self.0.take().map(|x| x.send(t));
    }

    pub fn is_closed(&self) -> bool {
        self.0.as_ref().map(|x| x.is_closed()).unwrap_or(true)
    }
}

impl<T> Receiver<T> {
    pub fn close(&mut self)  {
        self.0.complete.set(true);
        if let Some(waker) = self.0.cancel_waker.take(){
            waker.wake()
        }
    }
    
    /// Asset channel will not be closed.
    pub fn into_out(self) -> ChannelOut<T> {
        ChannelOut(self)
    }

    /// Map cancel as option.
    pub fn into_option(self) -> ChannelOutOrCancel<T> {
        ChannelOutOrCancel(self)
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.0.complete.set(true);
        if let Some(waker) = self.0.recv_waker.take(){
            waker.wake()
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.close()
    }
}

/// Future for a `!Send` oneshot channel being closed.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ChannelCancel<'a, T>(&'a mut Sender<T>);

impl<T> Future for ChannelCancel<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.0.0.poll_canceled(cx)
    }
}

/// Error for channel being closed.
#[derive(Debug)]
pub struct Canceled;

impl std::fmt::Display for Canceled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Oneshot channel closed.")
    }
}

impl std::error::Error for Canceled {}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Canceled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.recv(cx)
    }
}

impl<T> FusedFuture for Receiver<T> {
    fn is_terminated(&self) -> bool {
        self.0.complete.get()
    }
}

/// Channel output with cancellation asserted to be impossible.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ChannelOut<T>(pub(crate) Receiver<T>);

impl<T> Unpin for ChannelOut<T> {}

impl<T> Future for ChannelOut<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.0.recv(cx).map(|x| x.expect(CHANNEL_CLOSED))
    }
}

impl<T> FusedFuture for ChannelOut<T> {
    fn is_terminated(&self) -> bool {
        self.0.0.complete.get()
    }
}

/// Channel output with cancellation as `None`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ChannelOutOrCancel<T>(pub(crate) Receiver<T>);

impl<T> Unpin for ChannelOutOrCancel<T> {}

impl<T> Future for ChannelOutOrCancel<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.0.recv(cx).map(|x| x.ok())
    }
}

impl<T> FusedFuture for ChannelOutOrCancel<T> {
    fn is_terminated(&self) -> bool {
        self.0.0.complete.get()
    }
}

/// Channel output or ready immediately.
pub type MaybeChannelOut<T> = Either<ChannelOut<T>, Ready<T>>;


/// Channel output with cancellation as `None`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct InterpolateOut(pub(crate) Receiver<AsyncResult<()>>);

impl Future for InterpolateOut {
    type Output = AsyncResult<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.0.recv(cx).map(|x| match x {
            Ok(x) => x,
            Err(_) => Ok(()),
        })
    }
}

impl FusedFuture for InterpolateOut {
    fn is_terminated(&self) -> bool {
        self.0.0.complete.get()
    }
}

impl ChannelOutOrCancel<AsyncResult<()>> {
    pub(crate) fn into_interpolate_out(self) -> InterpolateOut {
        InterpolateOut(self.0)
    }
}
