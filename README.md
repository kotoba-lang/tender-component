# tender-component

Native Component Model engine adapter for Kototama.

`kototama` owns the tender contracts, admission envelope, provider boundary,
and aiueos grant translation. This repository owns the replaceable native
engine process and the narrow JSON protocol used to execute an already
admitted Component. No WASI directories, environment, arguments, or inherited
stdio are exposed.

The bundled Rust micro-TCB uses Wasmtime with a minimal feature set. Other
engines may implement the same host protocol without entering `kototama` core.
The CI matrix also executes a compiler-produced typed v0.3 Component in the
actual Cloudflare `workerd` binary. `jco` performs only the portable Component
to ESM/Core Wasm adaptation; workerd remains the engine. Its worker has an
explicit deny-all outbound service and receives only the named
`capability.acquire` and `log.append` providers. The same payload-carrying
Component is also executed through the Wasmtime and jco/Node adapters, including
grant, lease, byte/item quota, and persistent audit-receipt checks. Its type is
checked before transpilation so a v1 binary cannot pass under v0.3 metadata.

The same native binary has a resident mode for murakumo canaries.
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
