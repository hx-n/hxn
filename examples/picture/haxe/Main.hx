class Main {
	static function main() {
		// The library calls each function once for every pixel.
		var disc = new Picture(25, 9);
		disc.paint((x, y) -> {
			var dx = x - 12, dy = (y - 4) * 2;
			return dx * dx + dy * dy <= 36 ? 255 : 0;
		});
		var line = new Picture(25, 9);
		line.paint((x, y) -> y == 4 ? 255 : 0);

		// One object is passed to another.
		disc.lighten(line);
		show(disc);

		// What a function throws is what the caller catches.
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
		// A `Vec<u8>` arrives as `haxe.io.Bytes`.
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
