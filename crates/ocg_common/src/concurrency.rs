//! Concurrency utility types

use std::ops::Deref;

use arc_swap::ArcSwap;

use crate::prelude::*;

/// A cloneable, [`Sync`] handle that supports publishing a new version.
/// Optimized for cheap reads, few writes.
pub struct VersionedArc<T> {
    inner: Arc<VersionedArcInner<T>>,
    /// Last revision read by this specific handle instance
    last_revision: AtomicUsize,
}

struct VersionedArcInner<T> {
    // Holds (revision, inner data)
    swapper: ArcSwap<(usize, T)>,
    updater_mutex: Mutex<()>,
}

impl<T: Clone> VersionedArc<T> {
    /// Constructs a new cloneable config handle.
    /// It will return `true` from [`Self::was_updated()`].
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(VersionedArcInner {
                swapper: ArcSwap::new(Arc::new((1, inner))),
                updater_mutex: Default::default(),
            }),
            last_revision: AtomicUsize::new(0),
        }
    }

    /// Accesses the latest version of the inner object, do not hold onto the result for a long time.
    pub fn peek(&self) -> impl Deref<Target = T> {
        let data = self.inner.swapper.load();
        self.last_revision.store(data.0, AtomicOrdering::Release);
        Peek(data)
    }

    pub fn peek_if_changed(&self) -> Option<impl Deref<Target = T>> {
        let data = self.inner.swapper.load();
        let old_revision = self.last_revision.swap(data.0, AtomicOrdering::AcqRel);
        if old_revision != data.0 {
            Some(Peek(data))
        } else {
            None
        }
    }

    /// Returns if the config was updated since the last [`Self::read()`] or [`Self::peek()`]
    pub fn was_updated(&self) -> bool {
        self.inner.swapper.load().0 != self.last_revision.load(AtomicOrdering::Acquire)
    }

    /// Clones the reference to the latest version of the game configuration, and clears the was_updated status.
    pub fn read(&self) -> impl Deref<Target = T> + Clone {
        let data = self.inner.swapper.load_full();
        self.last_revision.store(data.0, AtomicOrdering::Release);
        Read(data)
    }

    /// Updates the configuration for all handles ever created out of the original config handle, and bumps the revision for change detection.
    /// Guarantees that only one updater runs concurrently, and config accesses are synchronized.
    /// `mark_self_as_updated` determines if this handle's revision should be bumped to avoid returning `true` from [`self.was_updated()`] for this update.
    pub fn update<F: FnOnce(&mut T)>(&self, updater: F, mark_self_as_updated: bool) {
        let _lock = self.inner.updater_mutex.lock().unwrap(); // Ensure only one update at a time
        let load = self.inner.swapper.load();
        let mut data = T::clone(&load.1);
        let new_revision = load.0.wrapping_add(1);
        drop(load);
        updater(&mut data);
        self.inner.swapper.store(Arc::new((new_revision, data)));
        if !mark_self_as_updated {
            self.last_revision.store(new_revision, AtomicOrdering::Release);
        }
    }
}

struct Peek<T>(arc_swap::Guard<Arc<(usize, T)>>);

impl<T> Deref for Peek<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0 .1
    }
}

#[derive(Clone)]
struct Read<T>(Arc<(usize, T)>);

impl<T> Deref for Read<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0 .1
    }
}
