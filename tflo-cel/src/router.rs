//! CEL-based routing for iterators.

use crate::context::IntoCelContext;
use crate::rule_engine::{CompiledRule, RuleEngine};

/// Extension trait for CEL-based routing on iterators.
///
/// # Examples
///
/// ```ignore
/// use tflo_cel::prelude::*;
///
/// let engine = RuleEngine::from_yaml(rules_yaml)?;
///
/// for (detection, matched_rules) in detections.into_iter().cel_route(&engine) {
///     for rule in matched_rules {
///         handle_action(&rule.action, &detection);
///     }
/// }
/// ```
pub trait CelRouterExt<T>: Iterator<Item = T> + Sized
where
    T: IntoCelContext,
{
    /// Route items through a rule engine.
    ///
    /// Each item is evaluated against all rules, and the iterator yields
    /// tuples of `(item, matched_rules)`.
    fn cel_route(self, engine: &RuleEngine) -> CelRouter<'_, Self, T>;

    /// Route items, yielding only those with at least one match.
    fn cel_route_matched(self, engine: &RuleEngine) -> CelRouterMatched<'_, Self, T>;
}

impl<I, T> CelRouterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    fn cel_route(self, engine: &RuleEngine) -> CelRouter<'_, Self, T> {
        CelRouter { iter: self, engine }
    }

    fn cel_route_matched(self, engine: &RuleEngine) -> CelRouterMatched<'_, Self, T> {
        CelRouterMatched { iter: self, engine }
    }
}

/// Iterator adapter that routes items through a rule engine.
pub struct CelRouter<'a, I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    iter: I,
    engine: &'a RuleEngine,
}

impl<I, T> std::fmt::Debug for CelRouter<'_, I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoCelContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelRouter")
            .field("iter", &self.iter)
            .field("rule_count", &self.engine.rule_count())
            .finish()
    }
}

impl<'a, I, T> Iterator for CelRouter<'a, I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    type Item = (T, Vec<&'a CompiledRule>);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let matches = self.engine.evaluate(&item);
        Some((item, matches))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Iterator adapter that yields only items with at least one rule match.
pub struct CelRouterMatched<'a, I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    iter: I,
    engine: &'a RuleEngine,
}

impl<I, T> std::fmt::Debug for CelRouterMatched<'_, I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoCelContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelRouterMatched")
            .field("iter", &self.iter)
            .field("rule_count", &self.engine.rule_count())
            .finish()
    }
}

impl<'a, I, T> Iterator for CelRouterMatched<'a, I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    type Item = (T, Vec<&'a CompiledRule>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let matches = self.engine.evaluate(&item);
            if !matches.is_empty() {
                return Some((item, matches));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cel_interpreter::Context;

    struct TestItem {
        value: i64,
    }

    impl IntoCelContext for TestItem {
        fn into_cel_context(&self) -> Context<'static> {
            let mut ctx = Context::default();
            let _ = ctx.add_variable("value", self.value);
            ctx
        }
    }

    #[test]
    fn test_cel_route() {
        let yaml = r#"
rules:
  - name: high
    condition: "value > 10"
    action: { type: alert }
  - name: low
    condition: "value < 5"
    action: { type: log }
"#;

        let engine = RuleEngine::from_yaml(yaml).expect("should parse");

        let items = vec![
            TestItem { value: 3 },
            TestItem { value: 7 },
            TestItem { value: 15 },
        ];

        let routed: Vec<_> = items.into_iter().cel_route(&engine).collect();

        assert_eq!(routed.len(), 3);

        // value=3 matches "low"
        assert_eq!(routed[0].1.len(), 1);
        assert_eq!(routed[0].1[0].name, "low");

        // value=7 matches nothing
        assert_eq!(routed[1].1.len(), 0);

        // value=15 matches "high"
        assert_eq!(routed[2].1.len(), 1);
        assert_eq!(routed[2].1[0].name, "high");
    }

    #[test]
    fn test_cel_route_matched() {
        let yaml = r#"
rules:
  - name: high
    condition: "value > 10"
    action: { type: alert }
"#;

        let engine = RuleEngine::from_yaml(yaml).expect("should parse");

        let items = vec![
            TestItem { value: 5 },
            TestItem { value: 15 },
            TestItem { value: 20 },
        ];

        let matched: Vec<_> = items.into_iter().cel_route_matched(&engine).collect();

        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].0.value, 15);
        assert_eq!(matched[1].0.value, 20);
    }
}
