# ADR 0010 — Independent Component host qualification

- Status: Accepted
- Date: 2026-07-26

## Decision

Kototama qualifies two independent implementations of the same closed
Component host contract:

- the Rust/Wasmtime micro-TCB;
- pinned `@bytecodealliance/jco` 1.25.2 under Node.js.

Both receive the same compiler-produced Component bytes, exact named WIT
imports, immutable execution identity, Aiueos decision, scoped ability,
resource declaration and host-managed lease. Neither host links WASI.
Concrete executable bytes are pinned separately in each full host receipt;
the execution identity binds the common versioned host contract.

The jco adapter is qualification evidence, not an ambient JavaScript escape
hatch. It is an executable with a closed JSON-line provider protocol. It
generates bindings only for admitted imports and maps only explicitly supported
interfaces. Unknown imports, malformed abilities and non-positive resource
bounds fail before instantiation.

## Cross-host invariant

`kototama.aiueos-adapter/portable-receipt` must be byte-value equivalent across
both hosts for the same run. It compares policy decision semantics, execution
identity, Component CID, exact imports and abilities, resource bounds,
capability consumption and outcome. Host runtime and executable hash remain in
the full receipt and must differ.

The integration qualification also proves equivalent rejection for:

- deny-by-default policy admission;
- live control-plane epoch revocation;
- one-shot capability exhaustion.

`test/tender/component_e2e_test.clj` is the normative executable evidence.
