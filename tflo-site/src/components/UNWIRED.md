# Unwired components — known orphan list

These components are real, work, and were written in the initial commit
— but no page or other component imports them. They are kept in-tree
under the "lost-not-dead" policy (see the parallel Rust case at
[`tflo-core/src/semantics.rs`](../../../tflo-core/src/semantics.rs)).

Discovered via StructureOS `SOS025` orphan-file diagnostics on the
2026-05-24 cleanup pass.

## Inventory

| Component                  | LOC | Probable purpose                                                                                      |
|----------------------------|----:|-------------------------------------------------------------------------------------------------------|
| `CelRulesEditor.tsx`       | 192 | Monaco-editor-backed CEL rules editor with JSON validation and an `onEvaluate` callback.              |
| `KnobPanel.tsx`            | 327 | 4-indicator (SMA/RSI/Bollinger/Cross) control panel with feed-source selector and play/pause.        |

Both components share the same `b0b3516 init` commit history and
nothing else. The pattern suggests a planned interactive playground
that was started and abandoned — the simpler `DemoChart` /
`PlaygroundChart` route shipped instead.

## When to recover

Wire `CelRulesEditor` into a new `/playground/cel-rules` page paired
with `tflo-cel` (or `tflo-rego`) wasm bindings when the CEL evaluation
demo becomes a priority.

Wire `KnobPanel` into a new `/playground/knobs` page next to the
existing `PlaygroundChart` to convert the static-config playground into
a tunable one.

## When to delete

Delete only after a deliberate product decision that these surfaces
are not wanted. Both files are self-contained and harmless to keep —
they don't pull in any code that isn't otherwise live, they don't ship
to the bundle (Astro tree-shakes unimported components), and the
header comments make future readers aware of their status.
