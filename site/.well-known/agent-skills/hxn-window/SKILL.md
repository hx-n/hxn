---
name: hxn-window
description: Open and drive native and browser windows from Haxe with the hxn-window haxelib (package `hxn.window`) on HashLink, Ash, hxcpp and CPPIA. Use when a Haxe program needs a window, its events (keyboard, mouse, touch, IME, resize, close), monitors, gamepads, the clipboard, the native window handles for creating a GPU surface, or the installed hxn window CLI tools.
---

# hxn-window

## Setup

```
haxelib install hxn-window
haxe -lib hxn-window -main Main -hl main.hl     # HashLink or Ash
haxe -lib hxn-window -main Main -cpp out        # hxcpp
```

The haxelib carries prebuilt native libraries for Linux x86-64 and ARM64,
macOS arm64 and Windows x86-64, and static libraries for Intel macOS (hxcpp
only), iOS, the iOS simulator and Android arm64 and x86-64; a program needs no Rust. For HashLink the `.hdll` is copied
beside the `.hl` output. On Linux, hxcpp links libudev for gamepads, so
install `libudev-dev`. Browser builds support threaded Ash wasm and hxcpp/Emscripten with or without threads. A
`haxelib git` install has no native libraries until a release ZIP's
`native/` directory is copied into it.

## Use

```haxe
import hxn.window.Window;
import hxn.window.WindowAttributes;

var attributes = new WindowAttributes();
attributes.title("Hello");
attributes.width(800);
attributes.height(600);
var window = Window.open(attributes);
attributes.close();
var open = true;
while (open) {
	switch window.wait(-1) {         // seconds; negative waits as long as it takes
		case Closed: open = false;
		case Resized(width, height): // haxe.Int64
		case KeyboardInput(deviceId, event, isSynthetic):
		case _:
	}
}
window.close();
```

- `WindowAttributes` setters (title, size, position, decorations,
  transparency, level, theme, icon, resizability, buttons) apply at
  `Window.open`.
- `window.poll()` returns the next event or `None` at once; `wait(timeout)`
  blocks, and the collector can run while it does. `Window.wake()`, from any
  thread, ends the current or next wait.
- `Event` cases include `Closed`, `Resized`, `Moved`, `Focused`,
  `ScaleFactorChanged`, `RedrawRequested`, `CursorMoved`, `MouseInput`,
  `MouseWheel`, `KeyboardInput`, `ModifiersChanged`, `Ime`, `DroppedFile`,
  `Touch` and gestures, and `None` when a wait times out.
- `Window` also has geometry and state getters and setters (`width`,
  `height`, `scaleFactor`, `setFullscreen`, `setMaximized`, ...), the cursor
  (`setCursorIcon`, `setCursorGrab`, `setCursorImage`), IME, monitors
  (`primaryMonitor`, `monitor(i)`) and the clipboard (`Window.clipboardText`).
- `Gamepad` is static: `Gamepad.count()`, `Gamepad.at(i)`, `Gamepad.poll()`
  for events; each pad has `button`, `axis`, `pressed` and `rumble`, and
  `vendorId()` and `productId()`, the USB ids (-1 where the platform does
  not report them). Tell the controller family from the vendor id, not from
  `name()`.

## Rules

- Every object (`Window`, `WindowAttributes`, `Monitor`, `Gamepad`, ...)
  holds a native resource until `close()`. Close each one.
- Platform errors are thrown as Haxe exceptions.
- 64-bit values such as `Resized`'s size and `raw` handles are `haxe.Int64`.

## GPU surfaces

`window.platform()` returns the platform: 1 AppKit, 2 Win32, 3 Xlib,
4 Wayland, 5 Android, 6 UIKit, 7 the browser canvas. `window.raw(0)` to `window.raw(3)` return the
native window and display handles. A GPU API's surface constructor takes the
code and the four handles in that order.

## Reference

The generated classes are committed under `binding/window/` in
https://github.com/hx-n/hxn-window: read them for every method's signature
and documentation. `examples/hello` is the program above;
`examples/headless` opens a headless window and checks its events.

## Working on this repository

The API is declared in xwindow; changing it means changing xwindow and the
revision `Cargo.toml` pins. Test with the hxn CLI:
`hxn check hl --vm ash` and `hxn check cpp cppia`, which run
`examples/headless` and compare the output across runtimes.

## Browser windows

The DOM agent is shared with xwindow. Ash loads it through `ash_host_agent`
using its wasm build and serving tools; threaded Emscripten uses a page
bridge. Unthreaded Emscripten runs the commands directly and never blocks
in `wait`; schedule turns with `hxn.native.Emscripten.later` and return to
the page. The GPU owns rendering and its drawing-buffer size; the window
agent keeps DOM events and CSS sizing on the page.

The installed package registers the `window` namespace with hxn 0.3.7 or later:
`hxn window doctor` lists its prebuilt targets. Run `hxn window check hl
--wasm --threads --browser --vm ash`, `hxn window check cpp --wasm --browser`,
or add `--threads` for threaded hxcpp. Native checks omit the wasm/browser
flags. These commands use packaged tools and checks, without Rust or a source
checkout. `hxn plugins` lists registrations.

For maintainers, `[package.metadata.hxn.cli]` registers before-check and before-build hooks
that regenerates the shared browser modules. `hxn package` preserves the
registration and ships the Haxe tool under `sources/`. See hxn's CLI extension
protocol and conventions when changing hooks. Build threaded packages with
`--release` when linking them with released GPU libraries.

## Redistribution

Retain the applicable [dependency licenses and notices](https://github.com/hx-n/hxn-window/blob/main/third-party/NOTICES.md)
when distributing binaries. Release packages include `third-party/`.

## Logging

Use core `hxn.Log` / `hxn::log`, with stable target `hxn.window`.
The application owns levels and the shared sink; libraries never replace it.
Generated adapters deliver native records after calls return. Custom frame loops
call `hxn.Loop.sync()`; otherwise idle background diagnostics need `hxn.Log.flush()`.
Keep real-time callbacks free of logging and leave returned errors as exceptions.
Read the [core logging contract](https://github.com/hx-n/hxn/blob/main/docs/design/logging.md).
