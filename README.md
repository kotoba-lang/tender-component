# tender-component

Native Component Model engine adapter for Kototama.

`kototama` owns the tender contracts, admission envelope, provider boundary,
and aiueos grant translation. This repository owns the replaceable native
engine process and the narrow JSON protocol used to execute an already
admitted Component. No WASI directories, environment, arguments, or inherited
stdio are exposed.

The bundled Rust micro-TCB uses Wasmtime with a minimal feature set. Other
engines may implement the same host protocol without entering `kototama` core.

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
