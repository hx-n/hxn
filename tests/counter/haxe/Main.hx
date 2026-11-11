/** The same checks on every runtime; `Counter` is that runtime's binding. **/
class Main {
	static var failed = 0;
	static var returned = false;

	static function check(what:String, ok:Bool) {
		if (!ok) {
			failed++;
			Sys.println('FAIL $what');
		}
	}

	static function thrown(body:() -> Void):Null<String> {
		try {
			body();
		} catch (e:haxe.Exception) {
			return e.message;
		}
		return null;
	}

	static function throws(what:String, body:() -> Void, expected:String) {
		var message = thrown(body);
		check('$what: $message', message != null && message.indexOf(expected) >= 0);
	}

	// Out of line, so no counter is left in a register or stack slot of `main`.
	static function churn(count:Int) {
		for (i in 0...count)
			new Counter(i);
	}

	static function collect() {
		#if hl
		hl.Gc.major();
		#elseif cpp
		cpp.vm.Gc.run(true);
		#end
	}

	static function main() {
		values();
		futures();
		continuations();
		returned = true;
	}

	static function values() {
		var counter = new Counter(40);
		check("add", counter.add(2) == 42);
		check("text in and out", counter.label("naïve ☃ 𝄞") == "naïve ☃ 𝄞=42");
		check("empty text", counter.label("") == "=42");
		check("bytes in", counter.sum(haxe.io.Bytes.ofHex("01020304ff")) == 265);
		check("empty bytes", counter.sum(haxe.io.Bytes.alloc(0)) == 0);

		throws("a failure is thrown", () -> counter.add(2147483647), "counter overflow");
		check("and leaves the value usable", counter.add(0) == 42);
		throws("a panic is thrown", Counter.fail, "failed on request");

		check("one counter alive", Counter.live() == 1);
		counter.close();
		check("close drops it", Counter.live() == 0);
		counter.close();
		throws("use after close", () -> counter.add(1), "closed");

		Counter.nap(1);
	}

	static function futures() {
		var square = Counter.squareLater(7, 100);
		check("a future starts pending", !square.isReady());
		check("await returns the value", square.await() == 49);
		check("then it is ready", square.isReady() && !square.isRejected());
		check("await again", square.await() == 49);

		var overflow = Counter.squareLater(100000, 1);
		throws("await throws a rejection", () -> overflow.await(), "counter overflow");
		check("which is ready and rejected", overflow.isReady() && overflow.isRejected());
		throws("await throws it again", () -> overflow.await(), "counter overflow");

		var abandoned = Counter.abandoned();
		throws("a future nothing will settle is cancelled", () -> abandoned.await(), "dropped before");

		check("a text result", Counter.greetLater("wörld ☃", 1).await() == "hello wörld ☃");

		var many = [for (i in 0...100) Counter.squareLater(i, 5)];
		var total = 0;
		for (future in many)
			total += future.await();
		check('many at once: $total', total == 328350);
	}

	// Delivered by this thread's event loop, which runs once `main` returns.
	static function continuations() {
		Counter.squareLater(9, 10).then(value -> {
			check("then delivers the value", value == 81);
			check("from the event loop", returned);
			Counter.squareLater(100000, 1).then(_ -> check("then took a rejection for a value", false), error -> {
				check('then delivers the rejection: ${error.message}', error.message.indexOf("counter overflow") >= 0);
				finish();
			});
		});
	}

	static function finish() {
		churn(500);
		collect();
		var left = Counter.live();
		check('the collector releases the rest, $left left', left < 50);

		Sys.println(failed == 0 ? 'PASS ($left of 500 unclosed counters left after a collection)' : 'FAILED $failed');
		Sys.exit(failed == 0 ? 0 : 1);
	}
}
