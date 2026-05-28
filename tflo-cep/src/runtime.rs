//! Iterator-adapter wrapper around the shared [`engine::Runtime`].
//!
//! The runtime itself lives in [`crate::engine`] and is reused by
//! `tflo-cep-wasm` for the push-based JS API. This module wraps it for
//! the iterator-based pull API ergonomic for synchronous Rust callers.

use std::collections::VecDeque;

use crate::engine::{self, Compiled};
use crate::pattern::{ArcEmit, ArcPredicate, ArcTimestamp, Pattern};

/// Iterator adapter that drives a [`Pattern`] over an underlying
/// `Iterator<Item = E>`, yielding one emit-output per successful match.
pub struct MatchPatternIter<I, E, M>
where
    I: Iterator<Item = E>,
    E: Clone + 'static,
    M: 'static,
{
    inner: I,
    engine: engine::Runtime<E, M, ArcPredicate<E>, ArcEmit<E, M>, ArcTimestamp<E>>,
    pending: VecDeque<M>,
    exhausted: bool,
    drained_negatives: bool,
}

impl<I, E, M> MatchPatternIter<I, E, M>
where
    I: Iterator<Item = E>,
    E: Clone + 'static,
    M: 'static,
{
    fn new(
        inner: I,
        compiled: Compiled<E, M, ArcPredicate<E>, ArcEmit<E, M>, ArcTimestamp<E>>,
    ) -> Self {
        Self {
            inner,
            engine: engine::Runtime::new(compiled),
            pending: VecDeque::new(),
            exhausted: false,
            drained_negatives: false,
        }
    }
}

impl<I, E, M> Iterator for MatchPatternIter<I, E, M>
where
    I: Iterator<Item = E>,
    E: Clone + 'static,
    M: 'static,
{
    type Item = M;

    fn next(&mut self) -> Option<M> {
        loop {
            if let Some(out) = self.pending.pop_front() {
                return Some(out);
            }
            if self.exhausted {
                if !self.drained_negatives {
                    let drained = self.engine.flush();
                    for m in drained {
                        self.pending.push_back(m);
                    }
                    self.drained_negatives = true;
                    continue;
                }
                return None;
            }
            match self.inner.next() {
                Some(event) => {
                    let emitted = self.engine.push(event);
                    for m in emitted {
                        self.pending.push_back(m);
                    }
                }
                None => self.exhausted = true,
            }
        }
    }
}

/// Iterator extension trait that enables `.match_pattern(p)` on any
/// `Iterator<Item = E>`.
pub trait PatternIter: Iterator + Sized {
    /// Apply a finalized [`Pattern`] to this iterator, yielding the emit
    /// closure's output per successful match.
    ///
    /// # Panics
    ///
    /// Panics if `pattern` has not been finalized via
    /// [`Pattern::emit`](crate::Pattern::emit). The builder API
    /// statically prevents this (`emit` returns the finalized form).
    fn match_pattern<M>(
        self,
        mut pattern: Pattern<Self::Item, M>,
    ) -> MatchPatternIter<Self, Self::Item, M>
    where
        Self::Item: Clone + 'static,
        M: 'static,
    {
        let compiled = pattern
            .take_compiled()
            .expect("pattern not finalized — call .emit(...) before .match_pattern");
        let _ = pattern; // suppress unused-mut warning when name_str is not used
        MatchPatternIter::new(self, compiled)
    }
}

impl<I: Iterator + Sized> PatternIter for I {}
