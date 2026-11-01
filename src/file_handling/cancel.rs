//! A cooperative "abandon this work" flag.
//!
//! A background writer cannot be interrupted mid-syscall, so cancelling it has
//! to be cooperative: the owner signals a [`CancelToken`], and the writer checks
//! it at the one point where giving up is still safe — immediately before it
//! renames its temp file over the target. That is what lets a force-quit stop a
//! stalled autosave from replacing a newer file the user has since opened, while
//! leaving every already-committed write untouched.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared, cheaply clonable flag that asks a background write to give up.
///
/// Cheap to clone (one atomic refcount), so the owner keeps one copy and hands
/// clones to the worker. A token starts uncancelled; cancelling is one-way.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A new, uncancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the work holding this token to abandon its write. Idempotent, and
    /// callable from any thread.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether [`cancel`](Self::cancel) has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clone_observes_the_cancel() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());

        token.cancel();

        assert!(clone.is_cancelled(), "the flag must be shared, not copied");
    }
}
