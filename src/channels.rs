//! Non-Send version of `futures_channels::oneshot`
use std::{cell::{Cell, RefCell}, pin::Pin, rc::Rc, task::{Context, Poll, Waker}};
use futures::Future;

#[derive(Debug)]
pub struct Sender<T>(Rc<Inner<T>>);

impl<T> Unpin for Sender<T> {}

#[derive(Debug)]
pub struct Receiver<T>(Rc<Inner<T>>);

impl<T> Unpin for Receiver<T> {}

#[derive(Debug)]
pub struct Lock<T>(RefCell<Option<T>>);

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

    pub fn cancellation(&mut self) -> ChannelCancel<T> {
        ChannelCancel(self)
    }
}

impl<T> Receiver<T> {
    pub fn close(&mut self)  {
        self.0.complete.set(true);
        if let Some(waker) = self.0.cancel_waker.take(){
            waker.wake()
        }
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

#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct ChannelCancel<'a, T>(&'a mut Sender<T>);

impl<T> Future for ChannelCancel<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.0.0.poll_canceled(cx)
    }
}

#[derive(Debug)]
pub struct Canceled;

impl<T> Future for Receiver<T> {
    type Output = Result<T, Canceled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.recv(cx)
    }
}
