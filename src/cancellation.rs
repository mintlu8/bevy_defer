//! Cancellation handles for `bevy_defer`.

use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Shared object for cancelling a running task.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Rc<Cell<bool>>);

/// Shared object for cancelling a running task that is `send` and `sync`.
#[derive(Debug, Clone, Default)]
pub struct SyncCancellation(Arc<AtomicBool>);

/// Shared object for cancelling a running task on drop.
#[derive(Debug, Clone, Default)]
pub struct CancelOnDrop(pub Cancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel()
    }
}

impl Cancellation {
    pub fn new() -> Cancellation {
        Cancellation(Rc::new(Cell::new(false)))
    }

    /// Cancel a running task.
    pub fn cancel(&self) {
        self.0.set(true)
    }

    /// Cancel a running task when the handle is dropped.
    pub fn cancel_on_drop(self) -> CancelOnDrop {
        CancelOnDrop(self)
    }
}

/// Shared object for cancelling a running task on drop that is `send` and `sync`.
#[derive(Debug, Clone, Default)]
pub struct CancelOnDropSync(pub SyncCancellation);

impl Drop for CancelOnDropSync {
    fn drop(&mut self) {
        self.0.cancel()
    }
}

impl SyncCancellation {
    pub fn new() -> SyncCancellation {
        SyncCancellation(Arc::new(AtomicBool::new(false)))
    }

    /// Cancel a running task.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed)
    }

    /// Cancel a running task when the handle is dropped.
    pub fn cancel_on_drop(self) -> CancelOnDropSync {
        CancelOnDropSync(self)
    }
}

/// Cancellation token for a running task.
#[derive(Debug, Clone)]
pub enum TaskCancellation {
    Unsync(Cancellation),
    Sync(SyncCancellation),
    None,
}

impl TaskCancellation {
    /// Returns `true` if cancelled.
    pub fn cancelled(&self) -> bool {
        match self {
            TaskCancellation::Unsync(cell) => cell.0.get(),
            TaskCancellation::Sync(b) => b.0.load(Ordering::Relaxed),
            TaskCancellation::None => false,
        }
    }
}

impl From<()> for TaskCancellation {
    fn from(_: ()) -> Self {
        TaskCancellation::None
    }
}

impl From<Cancellation> for TaskCancellation {
    fn from(val: Cancellation) -> Self {
        TaskCancellation::Unsync(val)
    }
}

impl From<&Cancellation> for TaskCancellation {
    fn from(val: &Cancellation) -> Self {
        TaskCancellation::Unsync(val.clone())
    }
}

impl From<SyncCancellation> for TaskCancellation {
    fn from(val: SyncCancellation) -> Self {
        TaskCancellation::Sync(val)
    }
}

impl From<&SyncCancellation> for TaskCancellation {
    fn from(val: &SyncCancellation) -> Self {
        TaskCancellation::Sync(val.clone())
    }
}

impl From<&CancelOnDrop> for TaskCancellation {
    fn from(val: &CancelOnDrop) -> Self {
        TaskCancellation::Unsync(val.0.clone())
    }
}

impl From<&CancelOnDropSync> for TaskCancellation {
    fn from(val: &CancelOnDropSync) -> Self {
        TaskCancellation::Sync(val.0.clone())
    }
}
