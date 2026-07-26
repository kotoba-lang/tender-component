# Kototama Wasmtime Component host

This is the native micro-TCB adapter used for effectful Component execution.
It links no WASI interfaces. The only host functions it can define are the
closed aiueos WIT imports, and each is bound to a validated ability descriptor.

The line-delimited JSON protocol is private to kototama.wasmtime-component.
It must not be exposed as a general component runner or used to pass ambient
environment, files, sockets, clock, random, process, or command-line access.
