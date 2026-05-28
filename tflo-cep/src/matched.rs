//! The `Match<E>` type — what `emit` closures receive.
//!
//! A `Match<E>` is the captured event history of a successfully matched
//! pattern. Positive steps (`when`, `then`) contribute one captured event
//! each; negative steps (`not_then`) contribute nothing because no closing
//! event arrived. The order is the order of capture.

use std::ops::Index;

/// A successfully matched pattern's captured events, in capture order.
///
/// Construct with [`Match::new`]; user code reaches it via the closure
/// passed to [`Pattern::emit`](crate::Pattern::emit). The closure receives
/// `&Match<E>` and produces the user's chosen output type (a typed event,
/// a `Signal`, a JSON value, anything that satisfies the emit signature).
///
/// # Indexing
///
/// `Match<E>` implements `Index<usize>` and `Index<&str>`. The string index
/// looks up by step name; the numeric index looks up by capture order.
pub struct Match<E> {
    name: String,
    events: Vec<(String, E)>,
}

impl<E> Match<E> {
    /// Construct a `Match` from its constituent (step-name, event) pairs.
    ///
    /// Public so connector crates and tests can synthesize one without
    /// going through the runtime.
    #[must_use]
    pub fn new(pattern_name: impl Into<String>, captures: Vec<(String, E)>) -> Self {
        Self {
            name: pattern_name.into(),
            events: captures,
        }
    }

    /// Pattern name (the value passed to [`Pattern::new`](crate::Pattern::new)).
    #[must_use]
    pub fn pattern_name(&self) -> &str {
        &self.name
    }

    /// The first captured event — the one that matched the initial `when`
    /// step. Always present in a `Match` (otherwise the pattern would not
    /// have started).
    ///
    /// # Panics
    ///
    /// Never — a `Match<E>` always carries at least the `when` capture.
    #[must_use]
    pub fn first(&self) -> &E {
        &self.events[0].1
    }

    /// The last captured event. For a positive sequence this is the closing
    /// `then` event; for a sequence ending in `not_then` this is the last
    /// preceding positive step (because no closing event arrived).
    ///
    /// # Panics
    ///
    /// Never — a `Match<E>` always carries at least one event.
    #[must_use]
    pub fn last(&self) -> &E {
        &self.events[self.events.len() - 1].1
    }

    /// All captured events in capture order.
    #[must_use]
    pub fn all(&self) -> Vec<&E> {
        self.events.iter().map(|(_, e)| e).collect()
    }

    /// Capture by step name. Returns `None` when no step with that name
    /// captured an event (e.g., a `not_then` step never has a capture).
    #[must_use]
    pub fn at(&self, step_name: &str) -> Option<&E> {
        self.events
            .iter()
            .find(|(n, _)| n == step_name)
            .map(|(_, e)| e)
    }

    /// Iterate captures as `(step_name, event)` pairs in capture order.
    pub fn named(&self) -> impl Iterator<Item = (&str, &E)> {
        self.events.iter().map(|(n, e)| (n.as_str(), e))
    }

    /// Number of captured events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True when no captured events. A constructed `Match<E>` is never
    /// empty; this exists for completeness with [`len`](Self::len).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl<E> Index<usize> for Match<E> {
    type Output = E;
    fn index(&self, idx: usize) -> &E {
        &self.events[idx].1
    }
}

impl<E: std::fmt::Debug> std::fmt::Debug for Match<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Match")
            .field("pattern", &self.name)
            .field("captures", &self.events)
            .finish()
    }
}
