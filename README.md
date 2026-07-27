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

```sh
clojure -M:test
cargo test --locked --manifest-path native/component-host/Cargo.toml
```
