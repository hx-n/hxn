class Main {
	static function main() {
		var tally = new Tally();

		// An async function returns a future immediately. These two
		// downloads run concurrently.
		var first = tally.fetch("a.txt", 3000);
		var second = tally.fetch("b.txt", 5000);
		Sys.println(first.await());
		Sys.println(second.await());

		// The cell is memory owned by the library, read here without a native call.
		var bytes = tally.bytes();
		Sys.println('${bytes.load()} bytes so far');

		// `await` throws the error an async function returns.
		try {
			tally.fetch("c.txt", -1).await();
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}

		// The running download holds its own reference to the tally, so
		// closing this object does not stop it. The cell outlives both.
		var last = tally.fetch("d.txt", 2000);
		tally.close();
		Sys.println(last.await());
		Sys.println('${bytes.load()} bytes in all');
	}
}
