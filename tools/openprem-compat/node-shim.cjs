/**
 * Launch an unchanged OpenPrem TypeScript/JS SDK agent against a chosen
 * controller. Mirrors shim.py for the Node SDK: it resolves `@openprem/sdk`
 * to the in-repo TypeScript SDK and repoints the Application's controller URL
 * at the target, without editing the example agent.
 *
 * Usage: node node-shim.cjs <controller_url> <agent_file.js>
 */
const Module = require('module');
const path = require('path');

const SDK = 'D:/DATA/WORK/OpenPrem/Apps/SolFlow/reference/open-prem-cleaning/sdk/typescript';
const controller = process.argv[2];
const agentFile = process.argv[3];

// Resolve the bare `@openprem/sdk` specifier to the in-repo SDK.
const origResolve = Module._resolveFilename;
Module._resolveFilename = function (request, ...rest) {
  if (request === '@openprem/sdk') return origResolve.call(this, SDK, ...rest);
  return origResolve.call(this, request, ...rest);
};

const sdk = require(SDK);
const Orig = sdk.Application;
sdk.Application = class extends Orig {
  constructor(opts) {
    super({ ...(opts || {}), controller });
  }
};

// Present the agent file with its own argv (drop the shim's leading args:
// node, node-shim.cjs, <controller>, <agentFile>).
const agentPath = path.resolve(agentFile);
process.argv = [process.argv[0], agentPath, ...process.argv.slice(4)];
require(agentPath);
