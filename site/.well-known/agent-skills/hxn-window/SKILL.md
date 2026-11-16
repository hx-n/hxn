---
name: hxn-window
description: Open and drive native windows from Haxe with the hxn-window haxelib (package `hxn.window`, over winit) on HashLink, Ash, hxcpp and CPPIA. Use when a Haxe program needs a window, its events (keyboard, mouse, touch, IME, resize, close), monitors, gamepads, the clipboard, or the native window handles for creating a GPU surface.
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
only), iOS, the iOS simulator and Android arm64; a program needs no Rust. For HashLink the `.hdll` is copied
beside the `.hl` output. On Linux, hxcpp links libudev for gamepads, so
install `libudev-dev`. wasm is not set up yet. A
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
  for events; each pad has `button`, `axis`, `pressed` and `rumble`.

## Rules

- Every object (`Window`, `WindowAttributes`, `Monitor`, `Gamepad`, ...)
  holds a native resource until `close()`. Close each one.
- Platform errors are thrown as Haxe exceptions.
- 64-bit values such as `Resized`'s size and `raw` handles are `haxe.Int64`.

## GPU surfaces

`window.platform()` returns the platform: 1 AppKit, 2 Win32, 3 Xlib,
4 Wayland, 5 Android, 6 UIKit. `window.raw(0)` to `window.raw(3)` return the
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
