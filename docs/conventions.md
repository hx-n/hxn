# Conventions

Read the rules for the area you are changing. [Design](design.md) describes
how the components work and why they use these approaches.

## Code and API rules

- [Crate boundaries](conventions/crates.md): dependencies, runtime isolation and generated files.
- [C ABI and generated names](conventions/boundary.md): boundary layouts, status values and naming constraints.
- [Rust API](conventions/rust-api.md): exposed functions, supported types, ownership, callbacks and asynchronous results.
- [Runtime bindings](conventions/bindings.md): Haxe wrappers, finalizers, collection and runtime-specific calls.
- [Threads and delivery](conventions/threads.md): runtime access, delivery ownership, waiting and atomic ordering.

## Validation and contribution

- [Testing](conventions/testing.md): runtime matrix, fixtures, examples, mobile tests, CI and Miri.
- [Releases and installation](conventions/releases.md): version tags, release archives and the installer.
- [Comments, commits and issues](conventions/workflow.md): documentation, commit messages and git-bug usage.
