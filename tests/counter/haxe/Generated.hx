/**
	Checks for the classes hxn-gen writes. The generated `Counter` is on the
	class path in place of a hand-written binding.
**/
class Generated {
	static var failed = 0;

	static function check(what:String, ok:Bool) {
		if (!ok) {
			failed++;
			Sys.println('FAIL $what');
		}
	}

	static function throws(what:String, body:() -> Void, expected:String) {
		var message:Null<String> = null;
		try {
			body();
		} catch (e:haxe.Exception) {
			message = e.message;
		}
		check('$what: $message', message != null && message.indexOf(expected) >= 0);
	}

	// Out of line, so no counter is left in a register or stack slot of `main`.
	static function churn(count:Int) {
		for (i in 0...count)
			new Counter(i);
	}

	// Objects passed to, returned from and consumed by the library.
	static function handles() {
		var a = new Counter(10);
		var b = new Counter(5);
		check("another object as a parameter", a.absorb(b) == 15);
		throws("the same object twice, when one use is exclusive", () -> a.absorb(a), "in use");
		a.swap(b);
		check("an object the library changes", a.add(0) == 5 && b.add(0) == 15);
		throws("null where an object is required", () -> a.absorb(null), "null pointer");
		check("and the object is usable afterwards", a.add(0) == 5);

		var before = Counter.live();
		var forked = a.fork();
		check("a method returns a new object", forked.add(1) == 6 && a.add(0) == 5 && Counter.live() == before + 1);
		check("an absent object in gives an absent one out", a.larger(null) == null && Counter.live() == before + 1);
		var larger = a.larger(b);
		check("a present one gives a present one", larger != null && larger.add(0) == 15);

		var live = Counter.live();
		check("a method that consumes its object returns", forked.intoValue() == 6);
		check("and the value is gone", Counter.live() == live - 1);
		throws("so the object is closed", () -> forked.add(1), "closed");
		forked.close();

		var tally = new Tally();
		tally.record(a);
		check("one interface takes another", tally.record(b) == 20 && tally.total() == 20);
		var total = tally.toCounter();
		check("and makes another", total.add(1) == 21);

		for (counter in [a, b, larger, total])
			counter.close();
		tally.close();
		check("everything made was released", Counter.live() == 0);
	}

	static function main() {
		var counter = new Counter(40);
		check("an int in and out", counter.add(2) == 42);
		check("text in and out", counter.label("naïve ☃ 𝄞") == "naïve ☃ 𝄞=42");
		check("empty text", counter.label("") == "=42");
		check("bytes in", counter.sum(haxe.io.Bytes.ofHex("01020304ff")) == 265);
		check("empty bytes", counter.sum(haxe.io.Bytes.alloc(0)) == 0);
		check("a bool out", counter.isPositive());
		check("a float in and out", counter.scaled(0.5) == 21);
		counter.set(5, true);
		check("a bool in and nothing out", counter.add(0) == -5 && !counter.isPositive());

		throws("a failure is thrown", () -> counter.add(-2147483647), "counter overflow");
		check("and leaves the value usable", counter.add(0) == -5);
		#if !wasm
		// A wasm library is built to abort on a panic, which ends the program.
		throws("a panic is thrown", Counter.fail, "failed on request");
		#end
		Counter.nap(1);

		check("a static function", Counter.live() == 1);
		counter.close();
		check("close drops the value", Counter.live() == 0);
		counter.close();
		throws("use after close", () -> counter.add(1), "closed");

		handles();

		#if emscripten
		// hxcpp's collector does not return from a forced collection under Emscripten.
		var collected = "no collection was run";
		#else
		churn(500);
		#if hl
		hl.Gc.major();
		#elseif cpp
		cpp.vm.Gc.run(true);
		#end
		var left = Counter.live();
		check('the collector releases the rest, $left left', left < 50);
		var collected = '$left of 500 unclosed counters left after a collection';
		#end

		Sys.println(failed == 0 ? 'PASS ($collected)' : 'FAILED $failed');
		Sys.exit(failed == 0 ? 0 : 1);
	}
}
