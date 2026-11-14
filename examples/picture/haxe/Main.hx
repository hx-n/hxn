import picture.Picture;

class Main {
	static function main() {
		// The library calls each callback once for every pixel.
		var disc = new Picture(25, 9);
		disc.paint((x, y) -> {
			var dx = x - 12, dy = (y - 4) * 2;
			return dx * dx + dy * dy <= 36 ? 255 : 0;
		});
		var line = new Picture(25, 9);
		line.paint((x, y) -> y == 4 ? 255 : 0);

		// One object is passed as a parameter to another object's method.
		disc.lighten(line);
		show(disc);

		// An exception thrown by a callback propagates to the caller of `paint`.
		try {
			disc.paint((x, y) -> {
				if (x == 5)
					throw new haxe.Exception('gave up at column $x');
				return 0;
			});
		} catch (error:haxe.Exception) {
			Sys.println(error.message);
		}

		try {
			disc.lighten(new Picture(2, 2));
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
	}

	static function show(picture:Picture) {
		// A `Vec<u8>` is returned as `haxe.io.Bytes`.
		var pixels = picture.pixels();
		var width = picture.width();
		var row = new StringBuf();
		for (i in 0...pixels.length) {
			row.add(pixels.get(i) > 127 ? "#" : ".");
			if (i % width == width - 1) {
				Sys.println(row.toString());
				row = new StringBuf();
			}
		}
	}
}
