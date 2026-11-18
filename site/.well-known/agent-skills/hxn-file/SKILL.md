---
name: hxn-file
description: Use or change hxn-file bundled reads, application saves, preferences, native roots and browser persistence across hxn runtimes.
---

Use `hxn.file.Storage` for bundled reads and application-scoped saves/preferences.
Operations return hxn.Future; use callbacks on the browser page thread. Close
Storage when finished. Started operations retain their state; a FileSource
borrows its Storage rather than taking ownership.

Read the [file API](https://github.com/hx-n/hxn-assets/blob/main/packages/hxn-file/README.md)
and [design](https://github.com/hx-n/hxn-assets/blob/main/docs/design.md) for roots,
limits, browser transport and persistence semantics. The compiler init macro
stages selected assets; an Android application host initializes ndk-context and
mobile packaging includes actual bundle/APK assets. Keep test hosts under tests/.

Native and browser writes copy inputs before returning. Browser results stay
page-owned until copied synchronously into Rust; never retain transient guest
pointers for asynchronous callbacks. Resolve paths through scoped roots and
settle writes only after the persistence operation commits.

Use this repository's git-bug tracker for its work. Follow the
[verification and packaging conventions](https://github.com/hx-n/hxn-assets/blob/main/docs/conventions.md).
Retain the [native dependency notices](https://github.com/hx-n/hxn-assets/blob/main/packages/hxn-file/third-party/NOTICES.md)
with redistributed binaries. A compile-only mobile result is not a runtime pass.

The URI backend supports hxn-assets RemoteSource HTTP(S) reads/chunks and WS(S)
messages. Reuse the existing browser mailbox and Future delivery; keep transient
guest pointers out of asynchronous callbacks. Native TLS uses verified roots;
browser trust and WebSocket header restrictions are browser policy. Keep URI
transport separate from scoped filesystem path validation.

## Logging

Diagnostics use core `hxn.Log` with target `hxn.file`.
The application owns `Log.level` and `Log.sink`; frame loops drain native
records at `hxn.Loop.sync()`, and idle background work uses `Log.flush()`.

## Repository CI

The [CI workflows](https://github.com/hx-n/hxn-assets/tree/main/.github/workflows)
run Rust/macro checks, native file/asset/remote fixtures on Linux, macOS and
Windows, and real Chrome pages in Ash and both hxcpp wasm modes. Reuse the
bounded runners under tests/ci; they are repository test tooling, not consumer
commands. For mobile bundle/APK reads, persistence across fresh app processes,
shared logging and real URI traffic, use the bounded
[mobile fixture](https://github.com/hx-n/hxn-assets/blob/main/tests/mobile/run.py)
with both hl (Ash AOT) and cpp on iOS simulator and Android emulator. Its app
hosts and Android context shim are test-only; keep them out of consumer tooling.
Physical-device lifecycle and network-transition checks are separate coverage.
