---
name: hxn-log
description: Use or change hxn core logging from Haxe and Rust, including shared sinks, severity filters, native record delivery and platform-console output. Use for app consoles or overlays and library diagnostics; logging is part of hxn, not a separate haxelib.
---

# Core logging

Haxe uses `hxn.Log`, `hxn.LogLevel` and `hxn.LogRecord`; Rust uses
`hxn::log`. They belong to hxn core. Add no standalone logging package.

```haxe
hxn.Log.level = Debug;
hxn.Log.sink = record -> trace('${record.origin} ${record.target}: ${record.message}');
hxn.Log.info("Scene ready", "game.scene");
```

```rust
hxn::log::debug!(target: "game.scene", "Scene ready");
```

Applications configure the shared level and sink on the Haxe logging thread.
Libraries use stable targets and leave that configuration to the application.
The default level is Info; Off disables records. Set `Log.sink = null` to restore
the platform console. Rust format arguments are lazy when disabled; Haxe callers
use `Log.enabled` before computing expensive arguments.

Native workers enqueue bounded owned records and retain no Haxe callback.
Generated bindings deliver them after boundary conversion and callback/exception
bookkeeping. Frame loops set `Loop.manual` and call `Loop.sync`; otherwise idle
background logs need an application-driven `Log.flush`. Logging adds no permanent
timer or thread. Keep real-time render callbacks free of logging.

Preserve target, severity, source file/line, Unix-millisecond timestamp, origin
and truncation metadata. Sink failures must preserve the original result or
exception. Queue overflow reports dropped records; records from separate native
libraries have no total chronological order. Standalone Rust programs use stderr;
third-party Rust subscribers and browser-agent diagnostics are not automatically
captured. Older bindings remain usable, but need regeneration to supply native
records to the shared sink.

When changing core delivery, exercise filtering, lazy formatting, metadata,
UTF-8 limits, overflow, recursive/failing sinks, multiple libraries, full DCE,
manual loops and supported runtime/threading modes. Adapter changes also follow
the core runtime and Miri verification rules.

Read the [logging design](https://github.com/hx-n/hxn/blob/main/docs/design/logging.md)
and [conventions](https://github.com/hx-n/hxn/blob/main/docs/conventions/logging.md).
The API remains in the core reference:
[Log](https://hxn.dev/api/hxn/hxn.Log/),
[LogLevel](https://hxn.dev/api/hxn/hxn.LogLevel/) and
[LogRecord](https://hxn.dev/api/hxn/hxn.LogRecord/).
