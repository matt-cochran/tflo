"use client";

/**
 * LOST-NOT-DEAD: orphaned React component, never wired into any page.
 *
 * 192-line Monaco-editor-backed CEL rules editor with JSON validation,
 * an `onEvaluate` callback, and a read-only toggle. Written in the
 * initial commit (single git history entry, `b0b3516 init`) but no
 * page or other component imports it.
 *
 * Probable original intent: paired with `KnobPanel.tsx` (also orphan)
 * for an interactive "build-your-own-pipeline" playground page that
 * was started but never finished. The simpler `DemoChart` /
 * `PlaygroundChart` approach shipped instead.
 *
 * Recovery options:
 *  1. Wire into a new `/playground/cel-rules` page paired with the
 *     tflo-rego or tflo-cel wasm bindings.
 *  2. Use as the editor surface in a future docs page about CEL rules.
 *
 * Discovered via StructureOS SOS025 on 2026-05-24 cleanup pass; left
 * in tree per "lost-not-dead" policy. See `tflo-core/src/semantics.rs`
 * for the parallel Rust case.
 */

import React, { useState, useCallback } from "react";
import Editor from "@monaco-editor/react";

interface CelRulesEditorProps {
  onEvaluate: (rules: Record<string, unknown>[]) => void;
  results?: { item_id: string; matched_rules: string[] }[];
  height?: number;
}

const DEFAULT_PLACEHOLDER = JSON.stringify(
  [
    {
      name: "example_rule",
      condition: "this.value > 50 && this.rsi > 70",
      label: "Overbought",
    },
  ],
  null,
  2,
);

export default function CelRulesEditor({
  onEvaluate,
  results = [],
  height: editorHeight = 300,
}: CelRulesEditorProps) {
  const [code, setCode] = useState<string>(DEFAULT_PLACEHOLDER);
  const [error, setError] = useState<string | null>(null);
  const [isReadOnly, setIsReadOnly] = useState(false);
  const [isEditorReady, setIsEditorReady] = useState(false);

  const handleEditorDidMount = useCallback(() => {
    setIsEditorReady(true);
  }, []);

  const handleChange = useCallback((value: string | undefined) => {
    setCode(value ?? "");
    setError(null);
  }, []);

  const handleEvaluate = useCallback(() => {
    setError(null);
    try {
      const parsed = JSON.parse(code);
      if (!Array.isArray(parsed)) {
        setError("Root value must be a JSON array of rule objects.");
        return;
      }
      for (const item of parsed) {
        if (typeof item !== "object" || item === null) {
          setError("Each rule must be a JSON object.");
          return;
        }
      }
      onEvaluate(parsed as Record<string, unknown>[]);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Invalid JSON. Please check your syntax.",
      );
    }
  }, [code, onEvaluate]);

  const handleToggleReadOnly = useCallback(() => {
    setIsReadOnly((r) => !r);
  }, []);

  const btnClass =
    "px-4 py-2 rounded text-sm font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-slate-900";

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-slate-700 bg-slate-900 p-4 text-white">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-400">
          CEL Rules Editor
        </h2>
        <label className="flex items-center gap-2 text-xs text-slate-400">
          <input
            type="checkbox"
            checked={isReadOnly}
            onChange={handleToggleReadOnly}
            aria-label="Toggle read-only mode"
            className="h-4 w-4 rounded border-slate-600 bg-slate-800 text-amber-500 focus:ring-amber-500 focus:ring-offset-slate-900"
          />
          Read-only
        </label>
      </div>

      {/* Monaco Editor */}
      <div className="overflow-hidden rounded-md border border-slate-700">
        <Editor
          height={editorHeight}
          language="json"
          theme="vs-dark"
          value={code}
          onChange={handleChange}
          onMount={handleEditorDidMount}
          options={{
            minimap: { enabled: false },
            lineNumbers: "on",
            scrollBeyondLastLine: false,
            fontSize: 13,
            fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
            readOnly: isReadOnly,
            tabSize: 2,
            automaticLayout: true,
            padding: { top: 8 },
          }}
          loading={
            <div className="flex h-full items-center justify-center text-sm text-slate-500">
              Loading editor...
            </div>
          }
        />
      </div>

      {/* Error message */}
      {error && (
        <div
          role="alert"
          className="rounded-md border border-red-800 bg-red-900/40 px-3 py-2 text-sm text-red-400"
        >
          {error}
        </div>
      )}

      {/* Evaluate button */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleEvaluate}
          disabled={!isEditorReady}
          aria-label="Evaluate rules"
          className={`${btnClass} border border-sky-600 bg-sky-600 text-white hover:bg-sky-500 disabled:cursor-not-allowed disabled:opacity-50`}
        >
          Evaluate
        </button>
        {!isEditorReady && (
          <span className="text-xs text-slate-500">Editor loading…</span>
        )}
      </div>

      {/* Results list */}
      {results.length > 0 && (
        <div className="flex flex-col gap-2">
          <h3 className="text-xs font-medium uppercase tracking-wider text-slate-400">
            Results ({results.length})
          </h3>
          <div className="max-h-48 overflow-y-auto rounded-md border border-slate-700 bg-slate-800">
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-slate-700 text-left text-slate-500">
                  <th className="px-3 py-2 font-medium">Item ID</th>
                  <th className="px-3 py-2 font-medium">Matched Rules</th>
                </tr>
              </thead>
              <tbody>
                {results.map((row, i) => (
                  <tr
                    key={row.item_id ?? i}
                    className="border-b border-slate-700/50 last:border-b-0"
                  >
                    <td className="px-3 py-2 font-mono text-white">
                      {row.item_id}
                    </td>
                    <td className="px-3 py-2">
                      {row.matched_rules.length > 0 ? (
                        <div className="flex flex-wrap gap-1">
                          {row.matched_rules.map((rule) => (
                            <span
                              key={rule}
                              className="inline-block rounded bg-amber-500/20 px-1.5 py-0.5 text-amber-400"
                            >
                              {rule}
                            </span>
                          ))}
                        </div>
                      ) : (
                        <span className="text-slate-500">—</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
