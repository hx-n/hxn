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

	static function numbers() {
		check("narrow numbers come back as they went",
			Numbers.byte(200) == 200 && Numbers.small(-100) == -100 && Numbers.word(60000) == 60000 && Numbers.short(-30000) == -30000);
		throws("one too large for its type is refused", () -> Numbers.byte(256), "out of range");
		throws("and one too small", () -> Numbers.small(-129), "out of range");
		throws("an unsigned one is not negative", () -> Numbers.word(-1), "out of range");
		check("sizes", Numbers.doubled(21) == 42 && Numbers.negated(5) == -5);
		throws("a size is not negative either", () -> Numbers.doubled(-1), "out of range");
		throws("a result that does not fit is refused too", () -> Numbers.doubled(0x7fffffff), "out of range");
		check("a number is converted before the library sees it", Numbers.sized(100) == 100);
		throws("and the library can still refuse it", () -> Numbers.sized(101), "too many");
		var all:UInt = Numbers.inverted(0);
		var low:UInt = Numbers.inverted(0xF0000000);
		check('an unsigned 32-bit number keeps its bits: $all', (all : Int) == -1 && (low : Int) == 0x0FFFFFFF && Std.string(all) == "4294967295");
		var big = haxe.Int64.make(0x12345678, 0x9ABCDEF0);
		check('a 64-bit number comes back as it went: $big', Numbers.wide(big) == big && Numbers.wide(-big) == -big && Numbers.wide(7) == 7);
		var squared = Numbers.squared(100000);
		check('and is not cut to 32 bits: $squared', haxe.Int64.toStr(squared) == "10000000000" && squared.high == 2);
		var flipped = Numbers.invertedWide(haxe.Int64.make(0, 0xFF));
		check('an unsigned one keeps its bits: $flipped', flipped.high == -1 && flipped.low == -256);
		var third = Numbers.third(1);
		check('a float of single precision: $third', Math.abs(third - 1 / 3) < 1e-7 && third != 1 / 3);
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

	// Cells both sides read and write with no call between them.
	static function atomics() {
		var counter = new Counter(0);
		var adds = counter.adds();
		counter.add(1);
		counter.add(1);
		check("a cell the library owns is read from here", adds.load() == 2);
		adds.store(10);
		counter.add(1);
		check("and written from here", adds.load() == 11);

		var locked = counter.locked();
		locked.store(true);
		check("a flag set here is seen by the library", counter.add(5) == 3 && adds.load() == 11);
		check("and read back", locked.exchange(false));
		check("and cleared", counter.add(5) == 8 && adds.load() == 12);

		var progress = new hxn.AtomicInt(0);
		var cancel = new hxn.AtomicBool(false);
		check("cells made here are lent to a call", counter.work(1000, progress, cancel) == 1000 && progress.load() == 1000);
		cancel.store(true);
		check("and a flag stops it", counter.work(1000, progress, cancel) == 0 && progress.load() == 1000);
		throws("null where a cell is required", () -> counter.work(1, null, cancel), "null pointer");

		counter.close();
		check("a cell outlives the object it came from", adds.add(1) == 12 && adds.load() == 13);

		#if !wasm
		// A thread of the library's own, with no Haxe call in progress.
		var job = new Job(100000);
		var done = job.done();
		var stop = job.stop();
		var waited = 0;
		while (done.load() < 5 && waited++ < 5000)
			Sys.sleep(0.001);
		stop.store(true);
		Sys.sleep(0.05);
		var at = done.load();
		Sys.sleep(0.05);
		check('a thread of the library\'s is watched and stopped through two cells: $at', at >= 5 && at < 100000 && done.load() == at);
		job.close();
		#end
	}

	// A value the library and several objects here hold at once.
	static function shared() {
		var pot = new Pot(10);
		var a = new Player(pot);
		var b = new Player(pot);
		check("a value kept by two others", a.pay(5) == 15 && b.pay(1) == 16 && pot.total() == 16);
		check("is one value", Pot.live() == 1 && a.paysInto(pot));

		var again = a.pot();
		check("the library hands it out again as a new object", again != pot && again.add(0) == 16);
		check("which holds the same value", again.same(pot) && pot.same(again) && pot.same(pot));
		var other = pot.split();
		check("and another value is not the same", !other.same(pot) && other.total() == 16 && Pot.live() == 2);
		check("nor is none", !pot.same(null));
		check("a value is told from another by the library too", !a.paysInto(other));
		throws("null where a shared object is required", () -> a.paysInto(null), "null pointer");

		pot.close();
		check("closing one object leaves the value to the rest", Pot.live() == 2 && a.pay(1) == 17 && again.total() == 17);
		throws("but closes that object", () -> pot.total(), "closed");

		check("no share is taken for none", a.moveTo(null) == null && a.paysInto(again));
		var left = a.moveTo(other);
		check("a share given to the library is kept", a.paysInto(other) && a.pay(1) == 17 && other.total() == 17);
		check("and the one it gave up comes back", left != null && left.same(again) && b.paysInto(left));
		check("an absent shared object in gives none out", a.fuller(null) == null);
		var fuller = a.fuller(again);
		check("a value returned plainly is shared by the adapter", fuller != null && fuller.total() == 17 && Pot.live() == 3 && !fuller.same(again));
		fuller.close();
		check("and released with its last holder", Pot.live() == 2);

		var weak = again.weak();
		var found = weak.upgrade();
		check("a weak reference finds a value still held", found != null && found.same(again));
		for (holder in [again, left, found])
			holder.close();
		check("the library's share keeps it alive", Pot.live() == 2 && b.pay(1) == 18);
		var last = weak.upgrade();
		check("and a weak reference still finds it", last != null && last.total() == 18);
		last.close();
		b.close();
		check("until the last holder is gone", Pot.live() == 1 && weak.upgrade() == null);
		weak.close();
		throws("a closed weak reference", () -> weak.upgrade(), "closed");

		a.close();
		check("a share outlives the object that gave it", other.total() == 17 && Pot.live() == 1);
		other.close();
		check("every pot was released", Pot.live() == 0);
	}

	// Functions of the program's, lent to a call.
	static function callbacks() {
		var counter = new Counter(3);
		check("a function is called by the library", counter.apply(4, value -> value * 2) == 48);
		var seen = [];
		Counter.each(3, (n, half) -> seen.push('$n:$half'));
		check('one that returns nothing, with two arguments: $seen', seen.join(" ") == "1:0.5 2:1 3:1.5");
		check("one that returns a bool", counter.test(value -> value == 48) && !counter.test(value -> value < 0));
		check("floats and bools going in", counter.scaledBy((half, odd) -> odd ? -1.0 : half * 3) == 72);
		check("one that takes nothing", Counter.ask(() -> 7) == 7);
		var made = Counter.fromFn(() -> 9);
		check("a call that is lent a function and returns an object", made.add(0) == 9);
		made.close();

		var thrown = new haxe.Exception("from the function");
		var caught:Null<haxe.Exception> = null;
		try {
			counter.apply(5, value -> {
				if (value >= 192)
					throw thrown;
				return value * 2;
			});
		} catch (error:haxe.Exception) {
			caught = error;
		}
		check("what a lent function throws reaches the caller as it was", caught == thrown);
		check("and the library stopped there", counter.add(0) == 192);
		throws("even when the library makes nothing of the failure", () -> Counter.attempt(() -> throw new haxe.Exception("ignored by the library")),
			"ignored by the library");
		throws("one that returns nothing can throw too", () -> Counter.each(3, (n, _) -> if (n == 2) throw new haxe.Exception("at two")), "at two");
		throws("an object is in use while its function runs", () -> counter.apply(1, value -> counter.add(1)), "in use");
		check("and usable afterwards", counter.add(0) == 192);
		check("nothing is left over for the next call", Counter.ask(() -> 1) == 1);
		counter.close();
	}

	// Results the library gives later: waited for here, and delivered.
	static function later() {
		check("a future is waited for", Later.square(7, 20).await() == 49);
		check("a text result", Later.greet("wörld ☃", 1).await() == "hello wörld ☃");
		check("work done a step at a time", Later.sum(20000).await() == 200010000);
		check("work that awaits other work", Later.combined(7, 100).await() == 5099);
		check("a bool and a float", Later.isEven(4).await() && !Later.isEven(3).await() && Later.half(5).await() == 2.5);
		throws("a failure is thrown from the wait", () -> Later.square(100000, 1).await(), "counter overflow");
		throws("a future nothing will settle is cancelled", () -> Later.abandoned().await(), "dropped before");
		throws("a function can refuse before there is a future", () -> Later.checked(-1), "negative limit");
		check("and give one otherwise", Later.checked(4).await() == 10);

		var before = Counter.live();
		var made = Later.counter(41).await();
		check("an object arrives later and is owned here", made.add(1) == 42 && Counter.live() == before + 1);
		made.close();
		var pot = Later.pot(5).await();
		check("so does a shared one", pot.add(1) == 6 && Pot.live() == 1);
		pot.close();
		check("and both are released", Counter.live() == before && Pot.live() == 0);

		check("a stream is read item by item", [for (item in Later.countUp(5, false)) item].join(",") == "1,2,3,4,5");
		check("one with nothing in it just ends", [for (item in Later.countUp(0, false)) item].length == 0);
		var got = [];
		throws("a stream that fails gives its items first", () -> {
			for (item in Later.countUp(3, true))
				got.push(item);
		}, "counter overflow");
		check('and they were $got', got.join(",") == "1,2,3");
		var total = 0;
		for (counter in Later.counters(4)) {
			total += counter.add(0);
			counter.close();
		}
		check("a stream of objects", total == 10 && Counter.live() == before);

		// Delivered where the app says, and to all of these in one batch.
		hxn.Loop.manual = true;
		var squared = -1;
		Later.square(6, 1).then(value -> squared = value);
		var refused = "";
		Later.square(100000, 1).then(_ -> {}, error -> refused = error.message);
		var items = [];
		var ended = false;
		Later.countUp(3, false).listen(item -> items.push(item), () -> ended = true);
		var ramp = Later.ramp(50);
		var levels = [];
		var stop = hxn.Reactive.effect(() -> levels.push(ramp.value));
		var label = Later.label(3);
		var waited = 0;
		while ((squared < 0 || refused == "" || !ended || ramp.peek() != 50 || label.peek() != "3") && waited++ < 5000) {
			hxn.Loop.sync();
			Sys.sleep(0.001);
		}
		check("a continuation is called with the value", squared == 36);
		check('or with the failure: $refused', refused.indexOf("counter overflow") >= 0);
		check('a listener gets every item and the end: $items', items.join(",") == "1,2,3" && ended);
		var rising = true;
		for (i in 1...levels.length)
			if (levels[i] <= levels[i - 1])
				rising = false;
		// A signal gives the latest value and nothing else, so how many of
		// the steps were seen depends on how fast the library got there.
		check('a signal is followed to its last value: $levels', levels[levels.length - 1] == 50 && rising);
		check("and one of text", label.peek() == "3");
		stop();
		hxn.Loop.manual = false;
	}

	// Out of line, as `churn` is. Returns the most that was held at once.
	static function pile(count:Int, size:Int):Float {
		var peak = 0.;
		for (i in 0...count) {
			new Blob(size);
			var held = HxnCounterMemory.meter.held;
			if (held > peak)
				peak = held;
		}
		return peak;
	}

	// Memory the collector cannot see, which the library declares. Returns
	// what it saw, for the summary.
	static function memory():String {
		var meter = HxnCounterMemory.meter;
		var mib = 1024 * 1024;
		check("nothing is held to begin with", meter.held == 0);
		var blob = new Blob(mib);
		check("what a value holds is counted", meter.held == mib && blob.size() == mib);
		blob.resize(3 * mib);
		check("and follows its size", meter.held == 3 * mib);
		var copy = blob.copy();
		check("a returned value is counted", meter.held == 6 * mib);
		copy.close();
		blob.close();
		check("close stops the count at once", meter.held == 0 && Blob.live() == 0);
		throws("a constructor that fails", () -> new Blob(-1), "negative size");
		check("counts nothing", meter.held == 0 && Blob.live() == 0);

		#if emscripten
		check("no collection can be run", !hxn.Gc.collect());
		return "no blobs piled";
		#else
		check("a collection can be run", hxn.Gc.collect());

		// Dropped without `close`, and nothing here asks for a collection.
		hxn.Gc.budget = 8 * mib;
		var peak = pile(400, mib);
		var left = Blob.live();
		check('dropped values are collected as they pile up: $left of 400 left, ${peak / mib} MiB held at most', peak <= 16 * mib && left <= 16);

		// Under the budget, so only the library's asking collects these.
		hxn.Gc.collect();
		hxn.Gc.budget = 1024 * mib;
		pile(4, mib);
		var before = Blob.live();
		Blob.crowded();
		hxn.Gc.check();
		check('a library asks for a collection: $before blobs before, ${Blob.live()} after', before >= 4 && Blob.live() < before);
		return '${peak / mib} MiB held at most by 400 unclosed blobs of 1 MiB under a budget of 8';
		#end
	}

	static function main() {
		var counter = new Counter(40);
		check("an int in and out", counter.add(2) == 42);
		check("text in and out", counter.label("naïve ☃ 𝄞") == "naïve ☃ 𝄞=42");
		check("empty text", counter.label("") == "=42");
		check("bytes in", counter.sum(haxe.io.Bytes.ofHex("01020304ff")) == 265);
		check("empty bytes", counter.sum(haxe.io.Bytes.alloc(0)) == 0);
		var filled = counter.fill(3);
		check("bytes out", filled.length == 3 && filled.get(0) == 42 && filled.get(2) == 42 && counter.sum(filled) == 126);
		check("no bytes out", counter.fill(0).length == 0);
		throws("a failure in place of bytes", () -> counter.fill(-1), "negative count");
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
		numbers();
		atomics();
		shared();
		callbacks();
		later();
		var held = memory();

		#if emscripten
		// hxcpp's collector does not return from a forced collection under Emscripten.
		var collected = 'no collection was run; $held';
		#else
		churn(500);
		hxn.Gc.collect();
		var left = Counter.live();
		check('the collector releases the rest, $left left', left < 50);
		var collected = '$left of 500 unclosed counters left after a collection; $held';
		#end

		Sys.println(failed == 0 ? 'PASS ($collected)' : 'FAILED $failed');
		Sys.exit(failed == 0 ? 0 : 1);
	}
}
