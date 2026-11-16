---
name: hxn-av
description: Audio and video in Haxe with the hxn-av haxelib (package `hxn.av`) on HashLink, Ash, hxcpp and CPPIA, using the platform's own codecs rather than FFmpeg. Use when a Haxe program plays a media file, works with video frames or PCM audio, applies an equalizer, encodes or decodes AAC or H.264, or reads or writes MP4.
---

# hxn-av

## Setup

```
haxelib install hxn-av
haxe -lib hxn-av -main Main -hl main.hl     # then: hl main.hl, or ash main.hl
haxe -lib hxn-av -main Main -cpp out        # then: out/Main
```

The haxelib carries prebuilt native libraries for Linux x86-64 and ARM64,
macOS arm64 and Windows x86-64, and static libraries for Intel macOS (hxcpp
only), iOS, the iOS simulator and Android arm64; a program needs no Rust. It
installs the `hxn` haxelib with it. wasm is not set up yet. A `haxelib git` install has
no native libraries until a release ZIP's `native/` directory is copied into
it.

Codecs are AAC-LC and H.264 Baseline: AVFoundation and VideoToolbox on
macOS, Media Foundation on Windows, GStreamer 1.20 or later on Linux. On
Linux GStreamer needs appsrc and appsink, `voaacenc`, `faad` or `fdkaacdec`,
`openh264enc`, a VA or OpenH264 decoder, and an audio output plugin for
playback. `VideoFrame`, `AudioData` and `AudioEqualizer` need none of them.

## Use

```haxe
import hxn.av.*;

var player = MediaPlayer.open("clip.mp4");
player.play();
while (player.state() != PlaybackState.Ended) {
	if (player.pollFrame()) {
		var frame = player.takeFrame();
		// draw frame, then:
		frame.close();
	}
}
player.close();
```

| Class | Use |
|---|---|
| `MediaPlayer` | `open(path)`, `play`, `pause`, `seek(seconds)`, `setVolume`, `setEqualizer`; `pollFrame` then `takeFrame` for the current video frame. |
| `VideoFrame` | Pixels in CPU memory: `create(bytes, init)`, `copyTo`, `crop`, `resize`, `blend`, `clone`, `retime`. |
| `AudioData` | PCM samples: `create(init)`, `copyTo`, `slice`, `gain`, `mix`, `clone`, `retime`. |
| `AudioEqualizer` | `create(bands)` (up to 16), `setBand(index, frequency, gainDb, q)`, `setPreamp`, `process(audio)`. |
| `AudioEncoder`, `VideoEncoder` | Encode a frame or a block of samples at a time. |
| `AudioDecoder`, `VideoDecoder` | `create`, `tryDecode(chunk)`, then `poll` and `read` for output; `finish` at the end. |
| `MediaDemuxer`, `MediaMuxer` | Read and write MP4 a packet at a time: `readVideoChunk`, `readAudioChunk`, `getVideoConfiguration`. |
| `MediaQueue` | A bounded queue of samples, frames, packets or bytes; a write returns false while it is full. |

`*Init` classes (`AudioDataInit`, `VideoFrameBufferInit`, ...) are the
constructors' arguments, built with `new`.

## Rules

- Every media object holds a native resource until `close()`; the collector
  does not release it. Close each one, including each frame `takeFrame` or a
  decoder returns. The `*Init` and options objects have `close()` too; close
  them after use, as `examples/check` does.
- Timestamps and durations are microseconds, as `haxe.Int64`. Counts and
  sizes are `haxe.Int64` too: `Int64.ofInt(1024)`.
- Failures are thrown as Haxe exceptions.
- `VideoFrame.copyTo` returns an `hxn.Future<PlaneLayouts>`: `await()` waits
  and returns the plane layouts, `then(callback)` takes a callback instead.

## Reference

The generated classes are committed under `binding/media/` in
https://github.com/hx-n/hxn-av: read them for every signature and its
documentation. `examples/check` makes a video frame, copies it back, and
filters a tone through an equalizer, with no audio or video device.

## Working on this repository

The API is declared in xavi; changing it means changing xavi and the
revision `Cargo.toml` pins. Test with the hxn CLI:
`hxn check hl --vm ash` and `hxn check cpp cppia`, which run
`examples/check` and compare the output across runtimes.
