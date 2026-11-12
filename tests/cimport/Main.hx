class Main {
	static function main() {
		var tally = new Tally("naïve ☃", Mode.Sum);
		tally.add(5);
		tally.add(-2);
		// Compared here rather than printed: a Windows console does not print
		// UTF-8 as it is.
		Sys.println('name kept: ${tally.name() == "naïve ☃"}, total ${haxe.Int64.toStr(tally.total())}');
		var added = tally.addBytes(haxe.io.Bytes.ofHex("010203"));
		Sys.println('added ${haxe.Int64.toStr(added)} bytes, total ${haxe.Int64.toStr(tally.total())}');
		Sys.println('described: ${tally.describe() == "naïve ☃: 9"}');
		Sys.println('summing: ${tally.modeOf() == Mode.Sum}');
		tally.setMode(Mode.Max);
		tally.add(40);
		tally.add(7);
		Sys.println('max: ${haxe.Int64.toStr(tally.total())}');

		// Haxe functions passed to C as a callback and its user data.
		var seen = [];
		var visited = tally.each(amount -> {
			seen.push(amount);
			return 0;
		});
		Sys.println('visited $visited: ${seen.join(",")}');
		Sys.println('stopped after ${tally.each(amount -> 1)}');
		try {
			tally.each(amount -> throw new haxe.Exception("no more"));
		} catch (error:haxe.Exception) {
			Sys.println('thrown: ${error.message}');
		}
		var lines = [];
		tally.report(line -> lines.push(line));
		Sys.println(lines.join(" | "));

		// A callback C keeps is a stream of what C calls it with, which ends
		// when the object is closed.
		var listened = new Tally("listened", Mode.Sum);
		var events = listened.listen();
		listened.add(3);
		listened.add(4);
		listened.close();
		var heard = [for (event in events) '${event.name} ${event.amount} ${haxe.Int64.toStr(event.total)}'];
		Sys.println('heard: ${heard.join(" | ")}');

		// A status code other than 0 is an exception.
		tally.setMode(Mode.Sum);
		try {
			tally.add(2147483647);
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
		// So is a constructor that returns NULL.
		try {
			new Tally("", Mode.Count);
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
		var fresh = new Tally("fresh", Mode.Count);
		Sys.println('empty: ${fresh.isEmpty()}');
		fresh.close();

		// Structs pass by value, and free functions are statics.
		var a:Point = {x: 1, y: 2};
		var b:Point = {x: 4, y: 6};
		Sys.println('distance: ${Tally.distance(a, b)}');
		var middle = Tally.midpoint(a, b);
		Sys.println('midpoint: ${middle.x}, ${middle.y}');
		Sys.println('version: ${haxe.Int64.toStr(Tally.version())}');

		// Values C writes through a pointer are returned.
		Sys.println('parsed: ${Tally.parse("42")}');
		try {
			Tally.parse("forty-two");
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
		var copy = tally.copy("copy");
		Sys.println('${copy.name()}: ${haxe.Int64.toStr(copy.total())}');
		copy.close();
		var origin = Tally.origin();
		Sys.println('origin: ${origin.x}, ${origin.y}');
		tally.close();
		hxn.Loop.exit(0);
	}
}
