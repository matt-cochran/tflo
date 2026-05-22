//! Public builder API for attaching [`CustomNode`] plugins to a graph.
//!
//! [`Comp::custom_node`] and [`Comp::custom_node1`] are the only public entry
//! points for inserting an external crate's runtime node into a `tflo` graph.
//! They live inside `tflo-core` so they may call the crate-private
//! `add_node_to_state` helper without exposing the internal [`Node`] enum.

use super::{Comp, Node, NodeId};
use crate::custom_node::{BoxedCustomNode, CustomNode, CustomNodeFactory};
use std::sync::Arc;

impl<R: 'static> Comp<R, f64> {
    /// Attach a multi-input [`CustomNode`] to the graph.
    ///
    /// `inputs` lists the computations whose values are passed — in order — to
    /// the node's [`eval`](CustomNode::eval) on every record. `factory`
    /// produces a fresh node instance for each compiled graph, so keyed
    /// execution gets independent per-key state.
    ///
    /// # Panics
    ///
    /// Panics if `inputs` is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use tflo_core::prelude::*;
    /// use tflo_core::custom_node::{require, CustomNode};
    ///
    /// struct Spread;
    /// impl CustomNode for Spread {
    ///     fn eval(&mut self, inputs: &[Computed]) -> Computed {
    ///         Ok(require(inputs, 0)? - require(inputs, 1)?)
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
    ///         Comp::custom_node(&[&ask, &bid], || Spread)
    ///     })
    ///     .collect();
    /// ```
    #[must_use]
    pub fn custom_node<F, N>(inputs: &[&Comp<R, f64>], factory: F) -> Comp<R, f64>
    where
        F: Fn() -> N + Send + Sync + 'static,
        N: CustomNode,
    {
        assert!(
            !inputs.is_empty(),
            "Comp::custom_node requires at least one input"
        );
        let state = &inputs[0].state;
        let input_ids: Vec<NodeId> = inputs.iter().map(|c| c.id).collect();
        let factory: CustomNodeFactory = Arc::new(move || {
            let node: BoxedCustomNode = Box::new(factory());
            node
        });
        Self::add_node_to_state(
            state,
            Node::Custom {
                inputs: input_ids,
                factory,
            },
        )
    }

    /// Attach a single-input [`CustomNode`] to the graph.
    ///
    /// Convenience wrapper around [`custom_node`](Self::custom_node) for nodes
    /// that consume only `self`.
    #[must_use]
    pub fn custom_node1<F, N>(&self, factory: F) -> Comp<R, f64>
    where
        F: Fn() -> N + Send + Sync + 'static,
        N: CustomNode,
    {
        Self::custom_node(&[self], factory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::TFlowBuilder;
    use crate::compile::{CompiledGraph, Computed};
    use crate::custom_node::require;
    use crate::iter_ext::TFlowIteratorExt;
    use crate::pipeline::Timestamped;

    /// Test node: emits the sum of all its inputs.
    struct SumNode;
    impl CustomNode for SumNode {
        fn eval(&mut self, inputs: &[Computed]) -> Computed {
            let mut sum = 0.0;
            for input in inputs {
                sum += (*input)?;
            }
            Ok(sum)
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
    impl CustomNode for RunningSum {
        fn eval(&mut self, inputs: &[Computed]) -> Computed {
            self.total += require(inputs, 0)?;
            Ok(self.total)
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
                Comp::custom_node(&[&a, &b], || SumNode)
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
            let node = Comp::custom_node(&[&x, &y], || SumNode);
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
        // custom-node input ids, g2 would re-read a+b and yield (3.0, 3.0).
        assert_eq!(item.value, (3.0, 30.0));
    }
}
