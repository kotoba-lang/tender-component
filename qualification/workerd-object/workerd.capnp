using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [
    (name = "deny", network = (allow = [])),
    (name = "application",
     worker = (
       compatibilityDate = "2026-07-27",
       globalOutbound = "deny",
       modules = [
         (name = "worker.mjs", esModule = embed "worker.mjs"),
         (name = "provider-capability.mjs", esModule = embed "provider-capability.mjs"),
         (name = "provider-object-store.mjs", esModule = embed "provider-object-store.mjs"),
         (name = "get/component.js", esModule = embed "get/component.js"),
         (name = "get/component.core.wasm", wasm = embed "get/component.core.wasm"),
         (name = "get/component.core2.wasm", wasm = embed "get/component.core2.wasm"),
         (name = "get/component.core3.wasm", wasm = embed "get/component.core3.wasm"),
         (name = "get/component.core4.wasm", wasm = embed "get/component.core4.wasm"),
         (name = "put/component.js", esModule = embed "put/component.js"),
         (name = "put/component.core.wasm", wasm = embed "put/component.core.wasm"),
         (name = "put/component.core2.wasm", wasm = embed "put/component.core2.wasm"),
         (name = "put/component.core3.wasm", wasm = embed "put/component.core3.wasm"),
         (name = "put/component.core4.wasm", wasm = embed "put/component.core4.wasm"),
         (name = "cas/component.js", esModule = embed "cas/component.js"),
         (name = "cas/component.core.wasm", wasm = embed "cas/component.core.wasm"),
         (name = "cas/component.core2.wasm", wasm = embed "cas/component.core2.wasm"),
         (name = "cas/component.core3.wasm", wasm = embed "cas/component.core3.wasm"),
         (name = "cas/component.core4.wasm", wasm = embed "cas/component.core4.wasm"),
       ],
     )),
  ],
  sockets = [
    (name = "http", address = "127.0.0.1:4176", http = (), service = "application"),
  ],
);
