//! Simple promise types that allow easily interacting with an asynchronous operation.

use std::future::Future;
use std::pin::Pin;

use anyhow::Error;
use tracing::error;

use crate::prelude::*;

/// Encompasses the operations of [`AsyncResult`] that do not depend on the held result type.
#[allow(clippy::wrong_self_convention)] // allow taking in mut self in is_ methods
pub trait GenericAsyncResult {
    /// Checks if the result is ready.
    #[must_use]
    fn is_ready(&mut self) -> bool;

    /// Returns if the inner result is Ok, or None if not ready yet.
    #[must_use]
    fn is_ok(&mut self) -> Option<bool>;

    /// Returns if the inner result is Err, or None if not ready yet.
    #[must_use]
    fn is_err(&mut self) -> Option<bool>;

    /// Checks if the result is available right now, returns a type-erased reference if it is.
    fn generic_poll(&mut self) -> Option<Result<(), &anyhow::Error>>;

    /// Waits for the result by blocking the current thread, wraps the error in a generic anyhow type.
    fn blocking_generic_wait(self: Box<Self>) -> Result<(), anyhow::Error>;

    /// Waits for the result by awaiting the inner future, wraps the error in a generic anyhow type.
    #[must_use]
    fn async_generic_wait(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;

    /// Spawns a new Tokio async task waiting for the result, and logs the error out if it is a failure.
    fn async_generic_log_when_fails(self: Box<Self>, context: &'static str);
}

/// A result holder that can await on an asynchronous operation happening in another thread or time.
#[derive(Debug)]
#[must_use]
pub enum AsyncResult<OkT: Send + 'static> {
    /// Not yet queried, or queried and not completed yet.
    Unresolved(AsyncOneshotReceiver<Result<OkT>>),
    /// Queried and completed.
    Resolved(Result<OkT>),
    /// Queried and the other end was missing.
    Aborted(anyhow::Error),
}

impl<OkT: Send + 'static> AsyncResult<OkT> {
    /// Constructs a new unresolved variant along with the channel to resolve it.
    pub fn new_pair() -> (Self, AsyncOneshotSender<Result<OkT>>) {
        let (tx, rx) = async_oneshot_channel();
        (Self::Unresolved(rx), tx)
    }

    /// Constructs a new pre-resolved variant.
    pub fn new_ok(val: OkT) -> Self {
        Self::Resolved(Ok(val))
    }

    /// Constructs a new pre-resolved variant.
    pub fn new_err(err: anyhow::Error) -> Self {
        Self::Resolved(Err(err))
    }

    /// Checks if the result is available right now, returns a reference if it is.
    pub fn poll(&mut self) -> Option<Result<&OkT, &anyhow::Error>> {
        match self {
            Self::Unresolved(recv) => match recv.try_recv() {
                Ok(v) => {
                    *self = Self::Resolved(v);
                    let Self::Resolved(v) = self else { unreachable!() };
                    Some(v.as_ref())
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(e @ tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    *self = Self::Aborted(anyhow::Error::from(e));
                    None
                }
            },
            Self::Resolved(val) => Some(val.as_ref()),
            Self::Aborted(err) => Some(Err(err)),
        }
    }

    /// Waits for the result by blocking the current thread until it is available. Do not use in async contexts.
    pub fn blocking_wait(self) -> Result<OkT> {
        match self {
            Self::Unresolved(chan) => match chan.blocking_recv() {
                Ok(v) => v,
                Err(e) => Err(anyhow::Error::from(e)),
            },
            Self::Resolved(val) => val,
            Self::Aborted(err) => Err(err),
        }
    }

    /// Waits for the result by awaiting the inner future. Do not use outside tokio contexts.
    pub async fn async_wait(self) -> Result<OkT> {
        match self {
            Self::Unresolved(chan) => match chan.await {
                Ok(v) => v,
                Err(e) => Err(anyhow::Error::from(e)),
            },
            Self::Resolved(val) => val,
            Self::Aborted(err) => Err(err),
        }
    }

    async fn async_generic_wait_impl(self) -> Result<()> {
        self.async_wait().await.map(|_| ()).map_err(anyhow::Error::from)
    }

    /// Spawns a new Tokio async task waiting for the result, and logs the error out if it is a failure.
    pub fn async_log_when_fails(self, context: &'static str) {
        tokio::spawn(async move {
            if let Err(e) = self.async_wait().await {
                error!("Error happened when resolving asynchronous result ({context}): {e}");
            }
        });
    }
}

impl<OkT: Send + 'static> GenericAsyncResult for AsyncResult<OkT> {
    fn is_ready(&mut self) -> bool {
        self.poll().is_some()
    }

    fn is_ok(&mut self) -> Option<bool> {
        self.poll().as_ref().map(Result::is_ok)
    }

    fn is_err(&mut self) -> Option<bool> {
        self.poll().as_ref().map(Result::is_err)
    }

    fn generic_poll(&mut self) -> Option<Result<(), &Error>> {
        self.poll().as_ref().map(|v| v.map(|_| ()))
    }

    fn blocking_generic_wait(self: Box<Self>) -> Result<()> {
        self.blocking_wait().map(|_| ()).map_err(anyhow::Error::from)
    }

    fn async_generic_wait(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>> {
        Box::pin(self.async_generic_wait_impl())
    }

    fn async_generic_log_when_fails(self: Box<Self>, context: &'static str) {
        self.async_log_when_fails(context)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn object_safe_sync_test() {
        let (r, tx) = AsyncResult::new_pair();
        tx.send(Ok(1i32)).unwrap();
        let mut rbox: Box<dyn GenericAsyncResult> = Box::new(r);
        let _ = rbox.is_ready();
        let _ = rbox.is_ok();
        let _ = rbox.is_err();
        rbox.generic_poll().unwrap().unwrap();
        rbox.blocking_generic_wait().unwrap();
    }

    #[tokio::test]
    async fn object_safe_async_test() {
        let (r, tx) = AsyncResult::new_pair();
        tx.send(Ok(1i32)).unwrap();
        let rbox: Box<dyn GenericAsyncResult> = Box::new(r);
        rbox.async_generic_wait().await.unwrap();
    }
}
