/* @ts-self-types="./solflow_compiler_wasm.d.ts" */
import * as wasm from "./solflow_compiler_wasm_bg.wasm";
import { __wbg_set_wasm } from "./solflow_compiler_wasm_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    analyze_source_json, compile_source_json, parse_source_json, run_source_json, version
} from "./solflow_compiler_wasm_bg.js";
