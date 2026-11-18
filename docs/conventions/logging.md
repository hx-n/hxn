# Logging

[Design](../design/logging.md) · [Conventions](../conventions.md)

- Library diagnostics use `hxn::log` in Rust and `hxn.Log` in Haxe. Keep
  command output and serialized protocols separate from diagnostics.
- Choose a stable subsystem target and appropriate severity. The application
  owns the shared filter and sink; a library does not replace them globally.
- Preserve source metadata. Rust format arguments are lazy when disabled;
  Haxe callers guard expensive argument creation with `Log.enabled`.
- Native threads enqueue records. Only a Haxe logging thread calls sinks,
  after boundary conversion and callback/exception bookkeeping have finished.
  Never store a host callback in the native logger.
- Keep records, queues and drain batches bounded. Release native locks before
  encoding or delivering a batch; overflow is reported through a drop count.
- Configure Haxe level/sink on the logging thread. Drive `Loop.sync` in a frame
  loop, or arrange `Log.flush` for idle background work. Do not add a permanent
  timer or delivery thread merely to collect logs.
- Do not log in a device render callback. Emit bounded state for a control
  thread to report, as described in [thread rules](threads.md).
- Keep the Haxe classes in the CLI's embedded support list. Exercise namespace
  functions and full DCE, older bindings without logging exports, failed calls,
  worker delivery, recursive/failing sinks, UTF-8 bounds and queue overflow.

The counter fixture covers native/shared-sink behavior across generated runtimes.
`tests/counter/haxe/LogOnly.hx` also checks namespace-only usage under full DCE;
`LogFrontend.hx` runs without native bindings on eval and JavaScript.
`python3 tests/counter/log-mobile.py <ios-sim|android> <hl|cpp> --device <id>`
runs shared Rust/Haxe capture and default output on a booted test device;
select an x86-64-compatible iOS simulator for hxcpp. Ash uses `--ash`, and Android
needs its NDK root and platform tools. These hosts remain test tooling.
ABI changes also run under Miri, with isolation disabled for wall-clock timestamps.
