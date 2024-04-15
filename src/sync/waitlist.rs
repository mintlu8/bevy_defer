use std::{cell::RefCell, task::{Context, Waker}};

/// A `!Send` internally mutable list of [`Waker`]s.
#[derive(Debug, Default)]
pub struct WaitList(RefCell<Vec<Waker>>);

impl WaitList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    pub fn push(&self, waker: Waker) {
        self.0.borrow_mut().push(waker)
    }

    pub fn push_cx(&self, cx: &Context) {
        self.0.borrow_mut().push(cx.waker().clone())
    }

    pub fn wake(&self) {
        self.0.borrow_mut().drain(..).for_each(|w| w.wake())
    }
}