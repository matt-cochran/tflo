"use client";

import React, { useState, useCallback } from "react";
import Editor from "@monaco-editor/react";
import LiveChart from "./LiveChart";
import type { Tick, Band } from "../lib/wasm";

export interface SignalBuilderProps {
  /** Optional initial source code in the editor */
  initialCode?: string;
  /** Called when the user clicks "Run" with the current editor value */
  onRun?: (code: string) => void;
}

const DEFAULT_CODE = `// Write signal detection logic here.
// The 'data' array contains incoming ticks (value, ts).
// Return an object with any derived indicators.

{
  // Example: detect when value crosses above 50
  signal: data.value > 50 ? "above_threshold" : null,
  customLabel: \`Tick \${data.ts}: \${data.value}\`
}`;

const DEFAULT_TICKS: Tick[] = [];
const DEFAULT_SMA: (number | null)[] = [];
const DEFAULT_BOLLINGER: (Band | null)[] = [];
const DEFAULT_CROSSES: { value: number; direction: string }[] = [];

export default function SignalBuilder({
  initialCode = DEFAULT_CODE,
  onRun,
}: SignalBuilderProps) {
  const [code, setCode] = useState<string>(initialCode);
  const [error, setError] = useState<string | null>(null);
  const [resultOutput, setResultOutput] = useState<string>("");
  const [isEditorReady, setIsEditorReady] = useState(false);

  // LiveChart data — currently stubbed. Will be wired to actual feed.
  const [ticks] = useState<Tick[]>(DEFAULT_TICKS);
  const [sma] = useState<(number | null)[]>(DEFAULT_SMA);
  const [bollinger] = useState<(Band | null)[]>(DEFAULT_BOLLINGER);
  const [crosses] = useState<{ value: number; direction: string }[]>(DEFAULT_CROSSES);

  const handleEditorDidMount = useCallback(() => {
    setIsEditorReady(true);
  }, []);

  const handleChange = useCallback((value: string | undefined) => {
    setCode(value ?? "");
    setError(null);
  }, []);

  const handleRun = useCallback(() => {
    setError(null);
    try {
      // Attempt to parse as JSON to validate; the actual evaluation
      // will be wired to the engine later.
      JSON.parse(code);
      setResultOutput("Signal evaluated successfully.");
      onRun?.(code);
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Failed to evaluate signal.";
      setError(message);
      setResultOutput(`Error: ${message}`);
    }
  }, [code, onRun]);

  const btnClass =
    "px-4 py-2 rounded text-sm font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-slate-900";

  return (
    <div className="flex flex-col gap-4 rounded-lg border border-slate-700 bg-slate-900 p-4 text-white">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-400">
          Signal Builder
        </h2>
        <button
          onClick={handleRun}
          disabled={!isEditorReady}
          aria-label="Run signal"
          className={`${btnClass} border border-sky-600 bg-sky-600 text-white hover:bg-sky-500 disabled:cursor-not-allowed disabled:opacity-50`}
        >
          Run
        </button>
      </div>

      {/* Split pane: Editor (left) + Chart (right) */}
      <div className="flex flex-col gap-4 lg:flex-row">
        {/* Left: Monaco Editor */}
        <div className="flex-1 overflow-hidden rounded-md border border-slate-700">
          <Editor
            height={320}
            language="javascript"
            theme="vs-dark"
            value={code}
            onChange={handleChange}
            onMount={handleEditorDidMount}
            options={{
              minimap: { enabled: false },
              lineNumbers: "on",
              scrollBeyondLastLine: false,
              fontSize: 13,
              fontFamily:
                "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
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

        {/* Right: LiveChart preview */}
        <div className="flex-1 overflow-hidden rounded-md border border-slate-700 bg-slate-800">
          <div className="border-b border-slate-700 px-3 py-2">
            <span className="text-xs font-medium uppercase tracking-wider text-slate-400">
              Preview
            </span>
          </div>
          <div className="p-2">
            <LiveChart
              data={ticks}
              sma={sma}
              bollinger={bollinger}
              crosses={crosses}
              width={500}
              height={280}
            />
          </div>
        </div>
      </div>

      {/* Error / Result output */}
      {error && (
        <div
          role="alert"
          className="rounded-md border border-red-800 bg-red-900/40 px-3 py-2 text-sm text-red-400"
        >
          {error}
        </div>
      )}
      {resultOutput && !error && (
        <div className="rounded-md border border-slate-700 bg-slate-800/50 px-3 py-2 text-sm text-slate-300">
          <pre className="whitespace-pre-wrap font-mono text-xs">
            {resultOutput}
          </pre>
        </div>
      )}
    </div>
  );
}
