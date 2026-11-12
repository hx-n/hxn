class Main {
	static function main() {
		var tally = new Tally();

		// An async function returns at once, with a future. These two
		// downloads run at the same time.
		var first = tally.fetch("a.txt", 3000);
		var second = tally.fetch("b.txt", 5000);
		Sys.println(first.await());
		Sys.println(second.await());

		// The cell is the library's own memory, read here without a call.
		var bytes = tally.bytes();
		Sys.println('${bytes.load()} bytes so far');

		// What an async function fails with is thrown by `await`.
		try {
			tally.fetch("c.txt", -1).await();
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}

		// The work holds a share of the tally, so closing this object does
		// not stop it, and the cell outlives both.
		var last = tally.fetch("d.txt", 2000);
		tally.close();
		Sys.println(last.await());
		Sys.println('${bytes.load()} bytes in all');
	}
}
