//! A simple mutation watcher, that allows detecting revision changes of the object held inside.
//! Designed for a multiplayer context, allowing local predicted changes.

use std::{cmp::Ordering, num::NonZeroU64, ops::Deref};

use crate::GameSide;

/// The revision tracking number type for [`MutWatcher`].
pub type RevisionNumber = NonZeroU64;

/// Holds `T` and keeps track of any mutations done to it.
#[derive(Clone, Debug, Hash)]
pub struct MutWatcher<T> {
    current_revision: RevisionNumber,
    predicted_revision: Option<RevisionNumber>,
    inner: T,
}

impl<T> MutWatcher<T> {
    /// The default revision for a brand new [`MutWatcher`].
    pub const INITIAL_REVISION_NUMBER: RevisionNumber = RevisionNumber::MIN;

    fn increment(num: &mut RevisionNumber) {
        *num = num.checked_add(1).unwrap();
    }

    /// Constructs a brand new [`MutWatcher`] assuming no previous revisions.
    pub fn new(inner: T) -> Self {
        Self {
            current_revision: Self::INITIAL_REVISION_NUMBER,
            predicted_revision: None,
            inner,
        }
    }

    /// Constructs a [`MutWatcher`] from a saved value and revision number.
    pub fn new_saved(inner: T, stored_revision: RevisionNumber) -> Self {
        Self {
            current_revision: stored_revision,
            predicted_revision: None,
            inner,
        }
    }

    /// Constructs a [`MutWatcher`] in predicted state, from a saved value, revision number and predicted revision number.
    pub fn new_predicted(inner: T, last_known_revision: RevisionNumber, predicted_revision: RevisionNumber) -> Self {
        Self {
            current_revision: last_known_revision,
            predicted_revision: Some(predicted_revision),
            inner,
        }
    }

    /// Constructs a [`MutWatcher`] with the same revision state as this one, but a different inner object.
    pub fn new_with_same_revision<U>(&self, inner: U) -> MutWatcher<U> {
        MutWatcher::<U> {
            current_revision: self.current_revision,
            predicted_revision: self.predicted_revision,
            inner,
        }
    }

    /// Extracts the inner stored value.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Accesses the inner value without mutating.
    #[inline]
    pub fn read(&self) -> &T {
        &self.inner
    }

    /// Accesses the last known non-predicted revision number.
    #[inline]
    pub fn last_known_revision(&self) -> RevisionNumber {
        self.current_revision
    }

    /// Accesses the predicted revision number with local changes, if currently predicting one.
    #[inline]
    pub fn predicted_revision(&self) -> Option<RevisionNumber> {
        self.predicted_revision
    }

    /// Accesses the locally current revision number, predicted or not.
    #[inline]
    pub fn local_revision(&self) -> RevisionNumber {
        self.predicted_revision.unwrap_or(self.current_revision)
    }

    /// Checks if the current state of this cell is a prediction.
    #[inline]
    pub fn is_prediction(&self) -> bool {
        self.predicted_revision.is_some()
    }

    /// Compares the revisions of two different cells.
    /// For the same local revision number, a non-predicted revision is newer than a predicted revision.
    /// Returns self <=> other.
    #[inline]
    pub fn compare_revisions<U>(&self, other: &MutWatcher<U>) -> Ordering {
        match self.local_revision().cmp(&other.local_revision()) {
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => match (self.is_prediction(), other.is_prediction()) {
                (false, false) | (true, true) => Ordering::Equal,
                (false, true) => Ordering::Greater,
                (true, false) => Ordering::Less,
            },
            Ordering::Less => Ordering::Less,
        }
    }

    /// Checks if the revision of this cell is newer than the other cell's revision.
    #[inline]
    pub fn is_newer_than<U>(&self, other: &MutWatcher<U>) -> bool {
        self.compare_revisions(other) == Ordering::Greater
    }

    /// Checks if the revision of this cell is newer than the other cell's revision.
    #[inline]
    pub fn is_older_than<U>(&self, other: &MutWatcher<U>) -> bool {
        self.compare_revisions(other) == Ordering::Less
    }

    /// Grants mutable access to the inner value and increases the current revision.
    /// Panics if in a predicted state.
    #[inline]
    pub fn mutate_stored(&mut self) -> &mut T {
        assert!(
            self.predicted_revision.is_none(),
            "Attempting to mutate_stored a MutWatcher in a predicting state"
        );
        Self::increment(&mut self.current_revision);
        &mut self.inner
    }

    /// Grants mutable access to the inner value and increases or calculates the current predicted revision.
    /// Make sure to send one mutation request to the server for each client call to this method, and server-side to call mutate_stored once for each received client mutation request (even if it fails).
    /// Otherwise [`Self::update_from_remote_revision`] will not work as intended.
    #[inline]
    pub fn mutate_predicted(&mut self) -> &mut T {
        match &mut self.predicted_revision {
            Some(rev) => {
                Self::increment(rev);
            }
            None => {
                self.predicted_revision = Some(self.current_revision.checked_add(1).unwrap());
            }
        }
        Self::increment(&mut self.current_revision);
        &mut self.inner
    }

    /// Calls [`Self::mutate_stored`] on the server and [`Self::mutate_predicted`] on the client.
    #[inline]
    pub fn mutate_sided(&mut self, side: GameSide) -> &mut T {
        match side {
            GameSide::Server => self.mutate_stored(),
            GameSide::Client => self.mutate_predicted(),
        }
    }

    /// Allows mutation access to the inner value without creating a new revision, use only for mutations that preserve the actual contents of the data (but for example optimize the layout in memory).
    pub fn mutate_without_revision(&mut self) -> &mut T {
        &mut self.inner
    }

    /// For client usage. Allows mutating the current value if the remote revision is newer than the current revision.
    /// When predicting revisions, updates if all our locally predicted mutations or more have resolved on the server.
    /// Returns a mutable reference if the mutation should happen, or None if it shouldn't.
    pub fn mutate_from_server_revision(&mut self, remote_revision: RevisionNumber) -> Option<&mut T> {
        let do_update = match self.predicted_revision {
            None => self.current_revision < remote_revision,
            Some(prev) => prev <= remote_revision,
        };
        if do_update {
            self.predicted_revision = None;
            self.current_revision = remote_revision;
            Some(&mut self.inner)
        } else {
            None
        }
    }
}

impl<T> Deref for MutWatcher<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, U> AsRef<T> for MutWatcher<U>
where
    T: ?Sized,
    <MutWatcher<U> as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}
