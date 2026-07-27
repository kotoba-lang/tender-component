#!/usr/bin/env node
// Independent JavaScript Component Model qualification host.
// It implements the same closed JSON-line protocol as the Wasmtime micro-TCB,
// but execution is through pinned @bytecodealliance/jco-generated bindings.

import { readFileSync, readSync, writeSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

let buffered = '';
function readLine() {
  for (;;) {
    const newline = buffered.indexOf('\n');
    if (newline >= 0) {
      const line = buffered.slice(0, newline);
      buffered = buffered.slice(newline + 1);
      return line;
    }
    const bytes = Buffer.alloc(4096);
    const n = readSync(0, bytes, 0, bytes.length, null);
    if (n === 0) throw new Error('provider closed protocol before responding');
    buffered += bytes.subarray(0, n).toString('utf8');
  }
}

function send(value) {
  writeSync(1, `${JSON.stringify(value)}\n`);
}

const supported = Object.freeze({
  'aiueos-clock-now': {
    operation: 'clock/now',
    specifier: 'kotoba:application/clock',
    exportName: 'now',
    resourceClass: 'NowCapability',
    issueName: 'issueNow',
    executeName: 'executeNow',
  },
});

function validateAbility(name, ability) {
  const binding = supported[name];
  if (!binding) throw new Error(`unrecognized aiueos Component import: ${name}`);
  if (!ability || ability.operation !== binding.operation ||
      !ability.target || !ability['audit-id'] ||
      !Number.isSafeInteger(ability['max-bytes']) || ability['max-bytes'] <= 0 ||
      !Number.isSafeInteger(ability['max-items']) || ability['max-items'] <= 0 ||
      !Number.isSafeInteger(ability['deadline-ms']) || ability['deadline-ms'] <= 0) {
    throw new Error(`import ${name} is bound to an invalid ability`);
  }
  return binding;
}

function providerSource(name, ability, binding, linear) {
  const exportName = binding.exportName;
  if (linear) return `
import { readSync, writeSync } from 'node:fs';
let buffered = '';
function line() {
  for (;;) {
    const n = buffered.indexOf('\\n');
    if (n >= 0) { const out = buffered.slice(0, n); buffered = buffered.slice(n + 1); return out; }
    const b = Buffer.alloc(4096);
    const count = readSync(0, b, 0, b.length, null);
    if (count === 0) throw new Error('provider closed protocol before responding');
    buffered += b.subarray(0, count).toString('utf8');
  }
}

export class ${binding.resourceClass} {}
export function ${binding.issueName}() { return new ${binding.resourceClass}(); }
export function ${binding.executeName}(cap, value) {
  if (!(cap instanceof ${binding.resourceClass})) throw new Error('invalid linear capability resource');
  writeSync(1, JSON.stringify({
    type: 'provider-call', import: ${JSON.stringify(name)},
    ability: ${JSON.stringify(ability)}, payload: { value: Number(value) }
  }) + '\\n');
  const response = JSON.parse(line());
  if (response.type !== 'provider-result' || response.import !== ${JSON.stringify(name)} ||
      !Number.isSafeInteger(response.value)) throw new Error('invalid provider-result response');
  return BigInt(response.value);
}
`;
  return `
import { readSync, writeSync } from 'node:fs';
let buffered = '';
function line() {
  for (;;) {
    const n = buffered.indexOf('\\n');
    if (n >= 0) { const out = buffered.slice(0, n); buffered = buffered.slice(n + 1); return out; }
    const b = Buffer.alloc(4096);
    const count = readSync(0, b, 0, b.length, null);
    if (count === 0) throw new Error('provider closed protocol before responding');
    buffered += b.subarray(0, count).toString('utf8');
  }
}
export function ${exportName}(value) {
  writeSync(1, JSON.stringify({
    type: 'provider-call',
    import: ${JSON.stringify(name)},
    ability: ${JSON.stringify(ability)},
    payload: { value: Number(value) }
  }) + '\\n');
  const response = JSON.parse(line());
  if (response.type !== 'provider-result' || response.import !== ${JSON.stringify(name)} ||
      !Number.isSafeInteger(response.value)) throw new Error('invalid provider-result response');
  return BigInt(response.value);
}
`;
}

function typedCapabilitySource(ability) {
  return `
export class Grant {
  constructor(operation) { this.operation = operation; }
}
export function acquire(request) {
  if (request !== 'clock-now') throw 'provider-failed';
  return new Grant(${JSON.stringify(ability.operation)});
}
`;
}

function typedClockSource(name, ability, lease) {
  return `
import { readSync, writeSync } from 'node:fs';
import { Grant } from './provider-capability.js';
let buffered = '';
let calls = 0;
function line() {
  for (;;) {
    const n = buffered.indexOf('\\n');
    if (n >= 0) { const out = buffered.slice(0, n); buffered = buffered.slice(n + 1); return out; }
    const b = Buffer.alloc(4096);
    const count = readSync(0, b, 0, b.length, null);
    if (count === 0) throw new Error('provider closed protocol before responding');
    buffered += b.subarray(0, count).toString('utf8');
  }
}
export function now(grant) {
  if (!(grant instanceof Grant) || grant.operation !== ${JSON.stringify(ability.operation)})
    throw 'provider-failed';
  calls += 1;
  if (calls > ${JSON.stringify(ability['max-items'])}) throw 'quota';
  writeSync(1, JSON.stringify({
    type: 'provider-call', import: ${JSON.stringify(name)},
    ability: ${JSON.stringify(ability)}, payload: null
  }) + '\\n');
  const response = JSON.parse(line());
  const proof = response['lease-proof'];
  if (response.type !== 'provider-result' || response.import !== ${JSON.stringify(name)} ||
      response['audit-id'] !== ${JSON.stringify(ability['audit-id'])} ||
      typeof response['audit-receipt'] !== 'string' || response['audit-receipt'].length === 0 ||
      !proof || proof.epoch !== ${JSON.stringify(lease.epoch)} ||
      proof['expires-at'] !== ${JSON.stringify(lease['expires-at'])} ||
      proof['observed-at'] < ${JSON.stringify(lease['not-before'])} ||
      proof['observed-at'] > ${JSON.stringify(lease['expires-at'])} ||
      !Number.isSafeInteger(response.payload)) throw 'provider-failed';
  return BigInt(response.payload);
}
`;
}

async function run(request) {
  if (request.type !== 'run') throw new Error('first protocol envelope must be a run request');
  if (!Number.isSafeInteger(request.fuel) || request.fuel <= 0 ||
      !Number.isSafeInteger(request['memory-pages']) || request['memory-pages'] <= 0) {
    throw new Error('resource bounds must be positive integers');
  }
  readFileSync(resolve(request.component)); // fail before tool invocation if absent
  const dir = mkdtempSync(join(tmpdir(), 'kototama-jco-'));
  try {
    writeFileSync(join(dir, 'package.json'), '{"type":"module"}\n');
    const mappings = [];
    const seen = new Set();
    const typed = request.lease !== undefined && request.lease !== null;
    for (const item of request.imports || []) {
      if (seen.has(item.name)) throw new Error('duplicate Component import');
      seen.add(item.name);
      const binding = validateAbility(item.name, item.ability);
      if (typed) {
        if (item.name !== 'aiueos-clock-now')
          throw new Error(`typed jco host does not implement ${item.name}`);
        const capabilityProvider = 'provider-capability.js';
        const clockProvider = 'provider-clock.js';
        writeFileSync(join(dir, capabilityProvider), typedCapabilitySource(item.ability));
        writeFileSync(join(dir, clockProvider),
                      typedClockSource(item.name, item.ability, request.lease));
        mappings.push(`aiueos:capability/capability=./${capabilityProvider}`);
        mappings.push(`aiueos:capability/clock=./${clockProvider}`);
      } else {
        const provider = `provider-${item.name}.js`;
        writeFileSync(join(dir, provider),
                        providerSource(item.name, item.ability, binding,
                                       request['capability-mode'] === 'linear-resource'));
        mappings.push(`${binding.specifier}=./${provider}`);
      }
    }
    const jco = resolve(new URL('../node_modules/.bin/jco', import.meta.url).pathname);
    const args = ['transpile', resolve(request.component), '-o', dir, '--name', 'component',
                  '--no-typescript', '--quiet'];
    for (const mapping of mappings) args.push('-M', mapping);
    const built = spawnSync(jco, args, { encoding: 'utf8' });
    if (built.status !== 0) throw new Error(`jco transpile failed: ${(built.stderr || built.stdout).trim()}`);
    const component = await import(`${pathToFileURL(join(dir, 'component.js')).href}?run=${Date.now()}`);
    const value = component.main();
    if (typeof value !== 'bigint') throw new Error('Component main must return s64');
    return Number(value);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

try {
  const request = JSON.parse(readLine());
  send({ type: 'result', value: await run(request) });
} catch (error) {
  send({ type: 'error', message: error instanceof Error ? error.message : String(error) });
}
