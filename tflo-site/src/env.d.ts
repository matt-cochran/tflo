/// <reference types="astro/client" />
/// <reference path="../.astro/types.d.ts" />

/** Ambient declaration for the wasm-pack generated glue module. */
declare module "/wasm/tflo.js" {
  const init: () => Promise<void>;
  export default init;
  export function compute_sma(input_json: string, config_json: string): string;
  export function compute_rsi(input_json: string, config_json: string): string;
  export function compute_bollinger(
    input_json: string,
    config_json: string,
  ): string;
  export function detect_cross(input_json: string, config_json: string): string;
  export function compute_indicator(
    input_json: string,
    config_json: string,
  ): string;
  export function evaluate_rules(
    rules_json: string,
    items_json: string,
  ): string;
  export function evaluate_rules_from_yaml(
    rules_yaml: string,
    items_json: string,
  ): string;
}
