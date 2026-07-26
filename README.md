# tender-component

Native Component Model engine adapter for Kototama.

`kototama` owns the tender contracts, admission envelope, provider boundary,
and aiueos grant translation. This repository owns the replaceable native
engine process and the narrow JSON protocol used to execute an already
admitted Component. No WASI directories, environment, arguments, or inherited
stdio are exposed.

The bundled Rust micro-TCB uses Wasmtime with a minimal feature set. Other
engines may implement the same host protocol without entering `kototama` core.

```sh
clojure -M:test
cargo test --locked --manifest-path native/component-host/Cargo.toml
```

