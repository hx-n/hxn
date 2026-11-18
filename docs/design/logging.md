# Logging

[Design](../design.md) · [Conventions](../conventions.md)

Logging belongs to hxn core. Haxe uses `hxn.Log`, `LogLevel` and `LogRecord`;
Rust uses `hxn::log`. No additional haxelib or crate is required.

```haxe
hxn.Log.level = Debug;
hxn.Log.info("Opened the scene", "game.scene");
hxn.Log.sink = record -> overlay.append(record.target, record.message);
// Restore the platform console:
hxn.Log.sink = null;
```

```rust
hxn::log::info!(target: "game.scene", "Opened scene {name}");
hxn::log::warn!("Asset {address} is unavailable");
```

Both sides use Off, Error, Warn, Info, Debug and Trace, in increasing verbosity;
Info is the default. Changing `Log.level` updates attached native loggers, and
new bindings receive the current setting when initialized. Off suppresses every
record. Rust macros test the level before evaluating format arguments or targets.
Haxe skips message-to-string conversion for disabled levels; use `Log.enabled`
before computing an expensive argument.

Records contain level, target, message, file, line, a Unix timestamp in
milliseconds, origin (`haxe` or `rust`) and a truncation flag. Rust targets
default to the emitting module path, Haxe targets to the calling class.
Haxe captures file and line through `haxe.PosInfos`. A shared target can identify
a subsystem across both languages. Records from different threads or native
libraries have no total chronological ordering.

## Delivery and output

An application configures the sink and level on its logging thread: the thread
that initializes `hxn.Log`. Haxe calls on that thread deliver immediately unless
`Loop.manual` batches them at an explicit sync;
Haxe worker calls queue until that thread calls `Log.flush` or `Loop.sync`.
Native worker threads only enqueue owned records. They never retain or invoke
a Haxe callback, touch its heap, start a delivery thread or wait for the sink.

Generated bindings register their pull functions when initialized. By default,
their public methods flush after result conversion and after callback/typed-exception
state has been restored. Futures flush after `await` consumes their result; inbox
delivery, `Loop.sync` and `Loop.exit` also flush. An application with a frame
loop sets `Loop.manual` and calls `Loop.sync` as usual: diagnostics then
batch at that sync, avoiding a native log poll after each frame-loop call.
If an otherwise idle worker produces logs,
the application must arrange periodic `Log.flush`; logging adds no timer or
thread and does not keep the application alive.

The sink runs outside native locks on the logging thread. Sink exceptions fall
back to console output and do not replace a native result or exception. A Haxe
log emitted by a sink goes directly to the console rather than recursively
calling the sink. Native records emitted during a sink remain for a later flush.
Sinks should avoid producing new diagnostics on every delivery.

With no sink, output is `[LEVEL] target: message` on the runtime's diagnostic
output (stderr, or the JavaScript console). Browser wasm output uses its host's
console routing. hxcpp Android writes to logcat's `hxn` tag with the record's
severity; other mobile output uses the application host's stderr routing.
A Rust program without an attached Haxe binding writes directly to stderr;
`hxn::log::set_level` changes its filter. No platform logging framework or
third-party Rust `log`/`tracing` subscriber is installed automatically.

Older packaged libraries without the pull exports remain usable. Regenerate
their bindings and rebuild with this hxn version to join the shared sink.

## Bounds and adapters

Messages are limited to 8192 UTF-8 bytes, targets to 256 and source paths to
1024. Truncation preserves UTF-8 boundaries and is reported in the record.
The Rust queue holds at most 256 records per linked logger; the Haxe worker
queue also holds at most 256. New records are dropped on overflow, preserving
earlier records, and a later flush emits a warning with the drop count.
Each native drain takes one bounded snapshot; records arriving during delivery
wait for another drain. Application-owned sink output is outside these bounds.

The queue and its wire encoding live in `hxn-abi`, which still has no dependency
or exported C symbol. `hxn` re-exports the Rust API. Generated adapters export
library-prefixed `log_level` and `log_take` functions, using the existing bytes
ownership rules. The wire batch contains a drop count and count of records,
followed by their fields; a null/empty byte result means no diagnostics. Haxe validates
the fixed limits while decoding. Static libraries may share one linked logger;
targets identify emitters independently of which adapter drains it.

Logging formats, allocates, locks and may write output. Keep it out of real-time
render callbacks; report bounded device state for a control thread to log.
