//! Common type aliases

// some of the imports here are not used yet, but are pre-defined for symmetry
#![allow(unused)]

// Anyhow error handling
pub use std::cell::{Cell, OnceCell, RefCell};
pub use std::sync::atomic::{Ordering as AtomicOrdering, *};
// Tokio and std MPSC channels
pub use std::sync::mpsc::{
    channel as std_unbounded_channel, sync_channel as std_bounded_channel, Receiver as StdUnboundedReceiver,
    Sender as StdUnboundedSender, SyncSender as StdBoundedSender,
};
// Common synchronization/cell types
pub use std::sync::{Arc, Mutex, MutexGuard, Once, OnceLock, RwLock, Weak};

pub use anyhow::{anyhow, bail, ensure, Context, Result};
// hashbrown Hash* types
pub use hashbrown::{HashMap, HashSet};
pub use tokio::sync::broadcast::{
    channel as async_broadcast_channel, Receiver as AsyncBroadcastReceiver, Sender as AsyncBroadcastSender,
};
pub use tokio::sync::mpsc::{
    channel as async_bounded_channel, unbounded_channel as async_unbounded_channel, Receiver as AsyncBoundedReceiver,
    Sender as AsyncBoundedServer, UnboundedReceiver as AsyncUnboundedReceiver, UnboundedSender as AsyncUnboundedSender,
};
// Tokio oneshot
pub use tokio::sync::oneshot::{
    channel as async_oneshot_channel, Receiver as AsyncOneshotReceiver, Sender as AsyncOneshotSender,
};
pub use tokio::sync::watch::{
    channel as async_watch_channel, Receiver as AsyncWatchReceiver, Sender as AsyncWatchSender,
};
