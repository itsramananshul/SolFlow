/* tslint:disable */
/* eslint-disable */

export function analyze_source_json(source: string): string;

export function compile_source_json(source: string): string;

/**
 * Tokenize + parse the given SOL source.
 */
export function parse_source_json(source: string): string;

/**
 * Version stamp the JS side can read to detect when it's loaded
 * an older WASM than the one it expected. Pinned to the crate
 * version in Cargo.toml.
 */
export function version(): string;
