# tender-component

Native Component Model engine adapter for Kototama.

`kototama` owns the tender contracts, admission envelope, provider boundary,
and aiueos grant translation. This repository owns the replaceable native
engine process and the narrow JSON protocol used to execute an already
admitted Component. No WASI directories, environment, arguments, or inherited
stdio are exposed.

The bundled Rust micro-TCB uses Wasmtime with a minimal feature set. Other
engines may implement the same host protocol without entering `kototama` core.
The CI matrix executes compiler-produced typed v0.3 Components through three
engine paths:

- the Rust/Wasmtime micro-TCB, which instantiates Component Model binaries;
- pinned `jco` bindings under Node.js, as an independent Component adapter;
- the actual Cloudflare `workerd` binary. `jco` performs only the portable
  Component-to-ESM/Core-Wasm adaptation; workerd remains the engine.

The qualified world contains no ambient WASI. Authority is split into the
individually named `identity.sign`, `identity.verify`, `hash.sha256`,
`http.post`, `http.get-stream`, `object-store.get-stream`,
`object-store.put-block`, `object-store.compare-and-set-ref`, `log.read`,
`log.append`, and `clock.now` operations. Every call first acquires a
host-owned grant for exactly one operation. Wasmtime and jco/Node additionally
recheck the scoped ability, lease epoch/expiry, item/byte quota, and persistent
audit receipt at the provider boundary.

The workerd qualification places eight payload-carrying Components (the five
core operations plus three object-store operations) in one isolate whose
`globalOutbound` service is deny-all. It exposes only their named WIT
interfaces and checks the exact results
`4,0,1,3,1,32,202,4`. The Component type is checked before transpilation so a
v1 binary cannot pass under v0.3 metadata.

The separate `tender-resident-component-host` binary provides the resident
mode for murakumo canaries, keeping that authority surface out of the typed
v0.3 protocol host.
It binds loopback only, verifies the exact Component bytes before readiness,
and emits node-key-signed, fsync'd execution receipts. It does not add a WASI
linker. With no capability configuration it is provider-free; with a
SHA-pinned configuration it links only the typed HTTP, append-only storage,
and LLM imports admitted for the cloud-itonami effect chain, recording every
call in the receipt.

```sh
clojure -M:test
cargo test --locked --manifest-path native/component-host/Cargo.toml
```
