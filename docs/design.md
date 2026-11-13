# Design

hxn generates adapters that let Haxe programs use a Rust library on HashLink,
Ash and hxcpp, natively and on wasm. Start with the architecture, then read the
pages for the component you are working on. [Conventions](conventions.md)
covers API rules, testing and contribution workflow.

## Architecture and values

- [Architecture and runtimes](design/architecture.md): crate layout, adapter responsibilities and runtime support.
- [Boundary types and failures](design/boundary.md): C representations, buffers, numeric conversions and exceptions.
- [Data and serialization](design/data.md): records, enums, collections, tuples and field defaults.
- [Handles and shared state](design/handles.md): ownership, borrowing, shared interfaces, weak references and atomics.
- [Garbage collection and native memory](design/memory.md): finalizers, accounting and collection requests.

## Calls and asynchronous work

- [Haxe callbacks](design/callbacks.md): callback arguments, ownership and exception handling.
- [Futures, streams and scheduling](design/async.md): asynchronous results, worker queues and delivery to Haxe.
- [Reactivity](design/reactivity.md): dependency tracking, computed values and following Rust signals.

## Generation and tools

- [Binding generation](design/generation.md): source parsing, manifests and generated adapters.
- [CLI builds and packaging](design/cli.md): commands, build settings, cross-compilation and haxelib packages.
- [Importing C libraries](design/c-import.md): header mapping, WebIDL review, callbacks and linking.
- [Website deployment](design/website.md): static assets, social previews and Cloudflare deployment.
