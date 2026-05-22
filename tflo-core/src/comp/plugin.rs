//! Public builder API for attaching [`Operator`](crate::operator::Operator) plugins to a graph.
//!
//! [`Comp::custom_node`] and [`Comp::custom_node1`] are the only public entry
//! points for inserting an external crate's runtime node into a `tflo` graph.
//! They live inside `tflo-core` so they may call the crate-private
//! `add_node_to_state` helper without exposing the internal [`Node`] enum.

use super::{Comp, Node, NodeId};
use crate::operator::{BoxedOperator, Operator, OperatorFactory};
use std::sync::Arc;

impl<R: 'static> Comp<R, f64> {
    /// Attach a multi-input [`Operator`](crate::operator::Operator) to the graph.
    ///
    /// A plugin node needs at least one input — that requirement is in the
    /// signature: `first` is the mandatory first input and `rest` is any
    /// further inputs. They are passed — `first`, then `rest` in order — to the
    /// operator's [`eval`](crate::operator::Operator::eval) on every record.
    /// `factory` produces a fresh operator instance for each compiled graph, so
    /// keyed execution gets independent per-key state.
    ///
    /// # Example
    ///
    /// ```
    /// use tflo_core::prelude::*;
    /// use tflo_core::operator::{require, Operator};
    ///
    /// struct Spread;
    /// impl Operator for Spread {
    ///     fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
    ///         let bid = match require(inputs, 0) { Ok(v) => v, Err(e) => return NodeOutput::computed(Err(e)) };
    ///         let ask = match require(inputs, 1) { Ok(v) => v, Err(e) => return NodeOutput::computed(Err(e)) };
    ///         NodeOutput::computed(Ok(ask - bid))
    ///     }
    /// }
    ///
    /// # struct Rec { ts: i64, bid: f64, ask: f64 }
    /// # let data: Vec<Rec> = vec![];
    /// let spreads: Vec<f64> = data.into_iter()
    ///     .tflo(|t| {
    ///         t.timestamp(|x| x.ts);
    ///         let bid = t.prop(|x| x.bid);
    ///         let ask = t.prop(|x| x.ask);
    ///         Comp::custom_node(&bid, &[&ask], || Spread)
    ///     })
    ///     .collect();
    /// ```
    #[must_use]
    pub fn custom_node<F, N>(
        first: &Comp<R, f64>,
        rest: &[&Comp<R, f64>],
        factory: F,
    ) -> Comp<R, f64>
    where
        F: Fn() -> N + Send + Sync + 'static,
        N: Operator,
    {
        // `first` guarantees at least one input — no empty-slice panic path.
        let state = &first.state;
        let input_ids: Vec<NodeId> = std::iter::once(first.id)
            .chain(rest.iter().map(|c| c.id))
            .collect();
        let factory: OperatorFactory = Arc::new(move || {
            let node: BoxedOperator = Box::new(factory());
            node
        });
        Self::add_node_to_state(
            state,
            Node::Plugin {
                inputs: input_ids,
                factory,
            },
        )
    }

    /// Attach a single-input [`Operator`](crate::operator::Operator) to the graph.
    ///
    /// Convenience wrapper around [`custom_node`](Self::custom_node) for operators
    /// that consume only `self`.
    #[must_use]
    pub fn custom_node1<F, N>(&self, factory: F) -> Comp<R, f64>
    where
        F: Fn() -> N + Send + Sync + 'static,
        N: Operator,
    {
        Self::custom_node(self, &[], factory)
    }

    /// Attach a multi-input [`Operator`](crate::operator::Operator) whose factory
    /// already yields a [`BoxedOperator`].
    ///
    /// This is the type-erased sibling of [`custom_node`](Self::custom_node). It
    /// exists for builders that pick the concrete operator type at *runtime* —
    /// e.g. dispatching on a [`Window`](crate::window::Window) discriminant to
    /// produce either a time-windowed or a count-windowed operator. Those are
    /// two distinct concrete types, so a `match` cannot return them through the
    /// monomorphic `N: Operator` bound of [`custom_node`](Self::custom_node);
    /// the factory must box each arm itself and return a uniform
    /// `BoxedOperator`.
    ///
    /// Behaviour is otherwise identical to [`custom_node`](Self::custom_node):
    /// `first` is the mandatory first input, `rest` is any further inputs, and
    /// the factory produces a fresh operator instance per compiled graph.
    #[must_use]
    pub fn custom_node_dyn<F>(
        first: &Comp<R, f64>,
        rest: &[&Comp<R, f64>],
        factory: F,
    ) -> Comp<R, f64>
    where
        F: Fn() -> BoxedOperator + Send + Sync + 'static,
    {
        // `first` guarantees at least one input — no empty-slice panic path.
        let state = &first.state;
        let input_ids: Vec<NodeId> = std::iter::once(first.id)
            .chain(rest.iter().map(|c| c.id))
            .collect();
        // The factory already yields a `BoxedOperator`, so — unlike
        // `custom_node` — no inner `Box::new` is needed here.
        let factory: OperatorFactory = Arc::new(factory);
        Self::add_node_to_state(
            state,
            Node::Plugin {
                inputs: input_ids,
                factory,
            },
        )
    }

    /// Attach a single-input type-erased [`Operator`](crate::operator::Operator)
    /// whose factory already yields a [`BoxedOperator`].
    ///
    /// Convenience wrapper around [`custom_node_dyn`](Self::custom_node_dyn) for
    /// operators that consume only `self`. See that method for why the
    /// `BoxedOperator` factory form exists.
    #[must_use]
    pub fn custom_node1_dyn<F>(&self, factory: F) -> Comp<R, f64>
    where
        F: Fn() -> BoxedOperator + Send + Sync + 'static,
    {
        Self::custom_node_dyn(self, &[], factory)
    }

    /// Create a sibling source node that extracts an `f64` directly from each
    /// input record, independent of `self`'s value.
    ///
    /// This is the [`Node::Prop`] equivalent of [`TFlowBuilder::prop`], but
    /// reachable from a `Comp` handle rather than the builder. It is useful for
    /// [`Operator`](crate::operator::Operator) plugins in extension crates that
    /// need a record-derived auxiliary input — e.g. a partition key fed as a
    /// second input to a multi-input [`custom_node`](Self::custom_node).
    ///
    /// The returned `Comp` lives in the same graph as `self`, so it can be
    /// passed straight to `custom_node`.
    ///
    /// [`TFlowBuilder::prop`]: crate::builder::TFlowBuilder::prop
    #[must_use]
    pub fn prop_from_record<F>(&self, extract: F) -> Comp<R, f64>
    where
        F: Fn(&R) -> f64 + Send + Sync + 'static,
    {
        Self::add_node_to_state(&self.state, Node::Prop(Arc::new(extract)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::TFlowBuilder;
    use crate::compile::{CompiledGraph, Computed, NodeOutput};
    use crate::iter_ext::TFlowIteratorExt;
    use crate::operator::require;
    use crate::pipeline::Timestamped;

    /// Test node: emits the sum of all its inputs.
    struct SumNode;
    impl Operator for SumNode {
        fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
            let mut sum = 0.0;
            for i in 0..inputs.len() {
                sum += match require(inputs, i) {
                    Ok(v) => v,
                    Err(e) => return NodeOutput::computed(Err(e)),
                };
            }
            NodeOutput::computed(Ok(sum))
        }
        fn name(&self) -> &str {
            "sum"
        }
    }

    /// Test node: emits the running sum of its single input.
    #[derive(Default)]
    struct RunningSum {
        total: f64,
    }
    impl Operator for RunningSum {
        fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
            NodeOutput::computed(match require(inputs, 0) {
                Err(e) => Err(e),
                Ok(v) => {
                    self.total += v;
                    Ok(self.total)
                }
            })
        }
    }

    #[derive(Clone)]
    struct Rec {
        ts: i64,
        a: f64,
        b: f64,
    }

    #[test]
    fn custom_node_two_inputs_via_tflo() {
        let data = vec![
            Rec {
                ts: 1,
                a: 1.0,
                b: 2.0,
            },
            Rec {
                ts: 2,
                a: 3.0,
                b: 4.0,
            },
            Rec {
                ts: 3,
                a: 10.0,
                b: 5.0,
            },
        ];
        let out: Vec<f64> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let a = t.prop(|x| x.a);
                let b = t.prop(|x| x.b);
                Comp::custom_node(&a, &[&b], || SumNode)
            })
            .collect();
        assert_eq!(out, vec![3.0, 7.0, 15.0]);
    }

    #[test]
    fn custom_node1_single_input_is_stateful() {
        let data = vec![
            Rec {
                ts: 1,
                a: 5.0,
                b: 0.0,
            },
            Rec {
                ts: 2,
                a: 7.0,
                b: 0.0,
            },
            Rec {
                ts: 3,
                a: 3.0,
                b: 0.0,
            },
        ];
        let out: Vec<f64> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.a).custom_node1(RunningSum::default)
            })
            .collect();
        assert_eq!(out, vec![5.0, 12.0, 15.0]);
    }

    /// `prop_from_record` builds a sibling source node that can be wired as a
    /// second input to a multi-input custom node — here it supplies `b` so the
    /// `SumNode` reads `a + b` without `b` ever being a builder-level `prop`.
    #[test]
    fn prop_from_record_feeds_a_custom_node() {
        let data = vec![
            Rec {
                ts: 1,
                a: 1.0,
                b: 2.0,
            },
            Rec {
                ts: 2,
                a: 3.0,
                b: 4.0,
            },
        ];
        let out: Vec<f64> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let a = t.prop(|x| x.a);
                let b = a.prop_from_record(|x| x.b);
                Comp::custom_node(&a, &[&b], || SumNode)
            })
            .collect();
        assert_eq!(out, vec![3.0, 7.0]);
    }

    /// Regression guard: `zip` must offset a custom node's input IDs, otherwise
    /// the second graph's node silently reads the first graph's values.
    #[test]
    fn zip_offsets_custom_node_input_ids() {
        #[derive(Clone)]
        struct Quad {
            ts: i64,
            a: f64,
            b: f64,
            c: f64,
            d: f64,
        }

        fn sum_graph(
            sel0: fn(&Quad) -> f64,
            sel1: fn(&Quad) -> f64,
        ) -> CompiledGraph<Quad, f64, Timestamped> {
            let mut builder = TFlowBuilder::<Quad>::new();
            let _ = builder.timestamp(|r| r.ts);
            let x = builder.prop(sel0);
            let y = builder.prop(sel1);
            let node = Comp::custom_node(&x, &[&y], || SumNode);
            let output_ids = vec![node.id];
            let nodes = builder.into_nodes();
            CompiledGraph::compile(Arc::new(|r: &Quad| r.ts), nodes, output_ids)
        }

        let g1 = sum_graph(|r| r.a, |r| r.b);
        let g2 = sum_graph(|r| r.c, |r| r.d);
        let mut combined = g1.zip(g2);

        let rec = Quad {
            ts: 1000,
            a: 1.0,
            b: 2.0,
            c: 10.0,
            d: 20.0,
        };
        let item = combined.step(&rec).expect("graph ready");
        // g1 sums a+b = 3.0; g2 sums c+d = 30.0. If zip failed to offset g2's
        // plugin node input ids, g2 would re-read a+b and yield (3.0, 3.0).
        assert_eq!(item.value, (3.0, 30.0));
    }
}
