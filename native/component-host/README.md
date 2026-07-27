# Kototama Wasmtime Component host

This is the native micro-TCB adapter used for effectful Component execution.
It links no WASI interfaces. The only host functions it can define are the
closed aiueos WIT imports, and each is bound to a validated ability descriptor.
The v0.3 binding is generated from the ABI repository's pinned WIT source and
implements identity, hash, HTTP, object-store, log, and clock interfaces.
Streaming bytes remain host-owned resources: polling, bounded reads,
cancellation, byte ceilings, and item ceilings are enforced inside this
process rather than delegated to a provider.

The line-delimited JSON protocol is private to kototama.wasmtime-component.
It must not be exposed as a general component runner or used to pass ambient
environment, files, sockets, clock, random, process, or command-line access.
