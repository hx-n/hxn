---
name: hxn-audio
description: Use or change hxn-audio audio output, voices, PCM streams, bus routing and effects in Haxe on native, mobile and browser runtimes, including its installed hxn audio CLI commands.
---

# hxn-audio

Use `-lib hxn-audio`; packaged adapters and the AudioWorklet DSP require no
Rust in an application. `new AudioOutput({})` opens default output;
`AudioOutput.offline({})` provides explicit render-to-Bytes for device-free
tests. Options are records, not constructed classes.

- `new AudioBuffer(bytes, rate, channels)` takes finite little-endian
  interleaved f32 PCM, mono or stereo. `output.play(buffer, options)` returns
  an independently controlled voice. The voice retains closed buffers.
- `output.stream(rate, channels, capacity, options)` takes bounded writes.
  `write` returns accepted frames. Retain/retry the suffix on a later tick;
  `finish` marks EOF only after all input was accepted. Do not busy-wait.
- With hxn-av installed, `Av.copy(data)` uses its existing format/planar
  conversion; `Av.buffer(data)` makes an immutable sound. Decode and copy
  outside rendering, and close media blocks/options. Inspect hxn-av's
  actual platform support before assuming its decoders run on the web.
  `DecoderStream` owns a stream and borrows the decoder/output; call `pump`
  from the event loop to retry partial writes and drain EOF. The repository
  playback test uses `--av` to validate the installed hxn-av decoder.
- Bus zero is master; `output.bus(index)` accesses the fixed bank. Routing
  is acyclic and output-local. Voices can move with `setBus(bus)`; buses
  offer gain/pan, low-pass and feedback delay. Close every handle.
- Browser creation returns while initialization is pending. Poll metrics
  for state/error, request playback activation when suspended, and use a
  secure isolated page. The agent resumes requested playback on click/key.
  `pause` prevents that retry until `resume`. Initialize an already-suspended
  AudioContext without awaiting another suspend request.
- Mobile applications own audio-session/focus/background policy and call
  pause/resume at lifecycle boundaries. Poll native device events and
  reopen with selectDevice; don't discard the mixer or pending music.
- Latency metrics are backend timestamp estimates, not acoustic loopback.
  A zero latency is unknown, not instantaneous output.

The installed CLI provides `hxn audio doctor`, source DSP builds and build/
package hooks. Application hosts own mobile startup and audio-session policy.
Playback runners and their iOS/Android/browser hosts belong under `tests/`,
excluded from haxelib assets and public CLI commands. For contributor runtime,
AAC and physical-device checks, read [testing](https://github.com/hx-n/hxn-audio/blob/main/tests/README.md); those
hosts validate this library and do not define standard hxn iOS tooling.

When editing, read docs/design.md and docs/conventions.md. Keep rendering
allocation/free/lock/callback-free; defer retirement to the controller.
Run the Rust tests and allocation audit, hxn check on every native runtime,
actual output checks on mobile and all three browser modes (including local
Safari when available), authorized Linux test hosts, and installed
package checks. Source hooks build the DSP for wasm/packaging with nightly
Rust and rust-src; haxelib consumers receive the compiled module.

## Redistribution

Retain the applicable [dependency licenses and notices](https://github.com/hx-n/hxn-audio/blob/main/third-party/NOTICES.md)
when distributing binaries, including the bundled unchanged MPL dependency source.
Release packages include `third-party/`.
