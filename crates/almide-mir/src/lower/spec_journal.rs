//! Undo journal for the speculation snapshot (`LowerCtx::speculate`).
//!
//! A speculative lowering attempt must leave no trace when it declines. The
//! snapshot used to clone every tracking map at entry. A large fn reaches
//! tens of thousands of attempts over maps of tens of thousands of entries,
//! so the copies dominated its compile time. A `Journaled` map instead
//! records, while a speculation is open, the prior state of every key it
//! writes. A declined attempt replays those records backwards, which
//! restores the same contents the clone restored. With no speculation open
//! a write records nothing.
//!
//! Reads go through `Deref` to the inner collection. Writes go through the
//! methods below, so no write can skip the journal; there is no `DerefMut`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::ops::Deref;

/// A collection whose single-key prior state can be read and put back.
pub(crate) trait Undo {
    type Key: Clone;
    type Prior;
    fn prior(&self, k: &Self::Key) -> Self::Prior;
    fn put_back(&mut self, k: Self::Key, prior: Self::Prior);
}

impl<K: Clone + Eq + Hash, V: Clone> Undo for HashMap<K, V> {
    type Key = K;
    type Prior = Option<V>;
    fn prior(&self, k: &K) -> Option<V> {
        self.get(k).cloned()
    }
    fn put_back(&mut self, k: K, prior: Option<V>) {
        match prior {
            Some(v) => self.insert(k, v),
            None => self.remove(&k),
        };
    }
}

impl<K: Clone + Ord, V: Clone> Undo for BTreeMap<K, V> {
    type Key = K;
    type Prior = Option<V>;
    fn prior(&self, k: &K) -> Option<V> {
        self.get(k).cloned()
    }
    fn put_back(&mut self, k: K, prior: Option<V>) {
        match prior {
            Some(v) => self.insert(k, v),
            None => self.remove(&k),
        };
    }
}

impl<K: Clone + Eq + Hash> Undo for HashSet<K> {
    type Key = K;
    type Prior = bool;
    fn prior(&self, k: &K) -> bool {
        self.contains(k)
    }
    fn put_back(&mut self, k: K, prior: bool) {
        if prior {
            self.insert(k);
        } else {
            self.remove(&k);
        }
    }
}

/// Where a speculation opened one map's journal: whether an outer
/// speculation already had it open, and the journal length at entry.
#[derive(Clone, Copy)]
pub(crate) struct Mark {
    nested: bool,
    len: usize,
}

/// A tracking collection with an undo journal (see the module doc).
pub(crate) struct Journaled<M: Undo> {
    inner: M,
    log: Option<Vec<(M::Key, M::Prior)>>,
}

impl<M: Undo + Default> Default for Journaled<M> {
    fn default() -> Self {
        Journaled { inner: M::default(), log: None }
    }
}

impl<M: Undo> Deref for Journaled<M> {
    type Target = M;
    fn deref(&self) -> &M {
        &self.inner
    }
}

impl<M: Undo> Journaled<M> {
    /// Open the journal for one speculation (nesting allowed).
    pub(crate) fn open(&mut self) -> Mark {
        match &self.log {
            Some(log) => Mark { nested: true, len: log.len() },
            None => {
                self.log = Some(Vec::new());
                Mark { nested: false, len: 0 }
            }
        }
    }

    /// Close the speculation opened at `mark`: on `keep == false` undo every
    /// write made since, newest first. The outermost close drops the journal;
    /// a nested one leaves its kept records for the enclosing speculation.
    pub(crate) fn close(&mut self, mark: Mark, keep: bool) {
        if let Some(log) = self.log.as_mut() {
            if !keep {
                while log.len() > mark.len {
                    let Some((k, prior)) = log.pop() else { break };
                    self.inner.put_back(k, prior);
                }
            }
        }
        if !mark.nested {
            self.log = None;
        }
    }

    /// Record `k`'s prior state when a speculation is open.
    fn note(&mut self, k: &M::Key) {
        if let Some(log) = self.log.as_mut() {
            log.push((k.clone(), self.inner.prior(k)));
        }
    }

    /// The inner collection, moved out (a finished fn hands its blocks to
    /// its `MirFunction`). Only outside any speculation.
    pub(crate) fn take_inner(&mut self) -> M
    where
        M: Default,
    {
        debug_assert!(self.log.is_none(), "take_inner inside a speculation");
        std::mem::take(&mut self.inner)
    }
}

impl<K: Clone + Eq + Hash, V: Clone> Journaled<HashMap<K, V>> {
    pub(crate) fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.note(&k);
        self.inner.insert(k, v)
    }
    pub(crate) fn remove(&mut self, k: &K) -> Option<V> {
        self.note(k);
        self.inner.remove(k)
    }
    pub(crate) fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        self.note(k);
        self.inner.get_mut(k)
    }
    pub(crate) fn entry(&mut self, k: K) -> std::collections::hash_map::Entry<'_, K, V> {
        self.note(&k);
        self.inner.entry(k)
    }
}

impl<K: Clone + Ord, V: Clone> Journaled<BTreeMap<K, V>> {
    pub(crate) fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.note(&k);
        self.inner.insert(k, v)
    }
    pub(crate) fn remove(&mut self, k: &K) -> Option<V> {
        self.note(k);
        self.inner.remove(k)
    }
    pub(crate) fn entry(&mut self, k: K) -> std::collections::btree_map::Entry<'_, K, V> {
        self.note(&k);
        self.inner.entry(k)
    }
}

impl<K: Clone + Eq + Hash> Journaled<HashSet<K>> {
    pub(crate) fn insert(&mut self, k: K) -> bool {
        self.note(&k);
        self.inner.insert(k)
    }
    pub(crate) fn remove(&mut self, k: &K) -> bool {
        self.note(k);
        self.inner.remove(k)
    }
    /// Make the set equal `to`, journaling each key that changes (a local
    /// save-and-restore of the whole set).
    pub(crate) fn restore_to(&mut self, to: HashSet<K>) {
        let gone: Vec<K> = self.inner.difference(&to).cloned().collect();
        for k in gone {
            self.remove(&k);
        }
        for k in to {
            if !self.inner.contains(&k) {
                self.insert(k);
            }
        }
    }
}
