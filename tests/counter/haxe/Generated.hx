/**
	Checks for the classes hxn-gen generates. The generated `Counter` is on
	the class path instead of a hand-written binding.
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

	static function functions() {
		check("a function is a static of the library's own class", Counter.version() == "counter 1");
		check("or of a class named for it", Tools.clamp(12, 0, 10) == 10 && Tools.halve(5) == 2.5);
		throws("one that fails", () -> Tools.clamp(1, 5, 0), "low is above high");
		var first = new Counter(4), second = new Counter(6);
		check("one that takes objects", Tools.total(first, second) == 10);
		var made = Tools.startingAt(9);
		check("one that makes an object", made.add(1) == 10);
		made.close();
		first.close();
		second.close();
	}

	// Plain data: an enum is passed as its discriminant, and a struct, an
	// option or a list is serialized to bytes on one side and deserialized on
	// the other.
	static function data() {
		check("an enum crosses as itself", Shapes.louder(Mode.Off) == Mode.Quiet && Shapes.louder(Mode.Quiet) == Mode.Loud);
		check("with the numbers the library gave it", (Mode.Quiet : Int) == 3 && (Mode.Loud : Int) == 9 && (Mode.Off : Int) == 10);
		check("and by reference", Shapes.named(Mode.Loud) == "Loud");
		throws("a number that names none is refused", () -> Shapes.louder((77 : Mode)), "number out of range");
		check("an enum that may be missing", Shapes.modeOf(9) == Mode.Loud && Shapes.modeOf(4) == null);

		var modes = Shapes.scaledModes([Mode.Quiet => 2, Mode.Loud => 3, Mode.Off => 0], 4);
		check("a map keeps its enum keys", modes.get(Mode.Quiet) == 8 && modes.get(Mode.Loud) == 12 && modes.get(Mode.Off) == 0);
		check("an empty map of enum keys", Lambda.count(Shapes.scaledModes([], 2)) == 0);
		var laterModes = Shapes.modesLater(modes).await();
		check("enum keys in a future", laterModes.get(Mode.Quiet) == 8 && laterModes.get(Mode.Loud) == 12);
		var mappedModes = Shapes.mappedModes(table -> {
			check("enum keys reach a callback", table.get(Mode.Quiet) == 2 && table.get(Mode.Loud) == 3);
			table.set(Mode.Off, 7);
			return table;
		});
		check("and come back", mappedModes.get(Mode.Off) == 7 && mappedModes.get(Mode.Loud) == 3);
		throws("an invalid enum key is refused", () -> Shapes.scaledModes([(77 : Mode) => 1], 1), "number out of range");
		check("a refused enum key leaves the next call usable", Shapes.scaledModes([Mode.Off => 3], 2).get(Mode.Off) == 6);

		var mid = Shapes.midpoint({x: 1, y: 2}, {x: 3, y: -4});
		check("a record in, by value and by reference, and one out", mid.x == 2 && mid.y == -1);

		var settings:Settings = {
			title: "naïve ☃",
			count: 41,
			wide: haxe.Int64.make(1, 2),
			ratio: 0.25,
			small: 200,
			flags: 0xF0000000,
			enabled: true,
			mode: Mode.Quiet,
			origin: {x: 0.5, y: -0.5},
			note: "kept",
			fallback: Mode.Off,
			path: [{x: 1, y: 1}, {x: 2, y: 2}, {x: 3, y: 3}],
			tags: ["a", "𝄞"],
			payload: haxe.io.Bytes.ofHex("00ff10"),
			grid: [[1, 2, 3], [], [-4]]
		};
		var described = Shapes.describe(settings);
		check('every kind of field arrives: $described',
			described == 'naïve ☃ x41 wide=4294967298 ratio=0.25 small=200 flags=4026531840 on Quiet at (0.5, -0.5) note=Some("kept") limit=None fallback=Some(Off) path=3 tags=a+𝄞 payload=[0, 255, 16] grid=[[1, 2, 3], [], [-4]]');
		var bumped = Shapes.bumped(settings);
		check("and comes back",
			bumped.title == "naïve ☃!" && bumped.count == 42 && bumped.wide == haxe.Int64.make(257, 2) && bumped.ratio == 0.25 && bumped.small == 200
			&& bumped.enabled && bumped.mode == Mode.Loud && bumped.origin.x == 0.5 && bumped.origin.y == -0.5);
		check("with what may be missing", bumped.note == "KEPT" && bumped.limit == null && bumped.fallback == Mode.Off);
		check("an unsigned field keeps its bits", (bumped.flags : UInt) == (0xF0000000 : UInt));
		check("and its lists",
			bumped.path.length == 3 && bumped.path[0].x == 3 && bumped.path[2].x == 1 && bumped.tags.join(",") == "a,𝄞,bumped"
			&& bumped.payload.toHex() == "00ff10" && bumped.grid.length == 3 && bumped.grid[0].join(",") == "1,2,3" && bumped.grid[1].length == 0
			&& bumped.grid[2][0] == -4);
		check("the record that went in is as it was", settings.count == 41 && settings.tags.length == 2);

		var preset = Shapes.preset(Mode.Loud);
		check("a record the library made",
			preset.title == "preset" && preset.wide == haxe.Int64.make(-1, -1) && preset.small == 255 && preset.mode == Mode.Loud && preset.note == null
			&& preset.limit == 7 && preset.fallback == Mode.Quiet && preset.path.length == 0 && preset.tags[1] == "naïve ☃"
			&& preset.payload.toHex() == "00ff" && preset.grid[1].length == 0);
		throws("or a failure in its place", () -> Shapes.preset(Mode.Off), "nothing is preset for off");
		settings.small = 256;
		throws("a field that does not fit its number is refused", () -> Shapes.describe(settings), "number out of range");
		settings.small = 0;
		settings.title = null;
		throws("and so is text that is missing", () -> Shapes.describe(settings), "null where text is needed");
		throws("and a record that is", () -> Shapes.describe(null), "null where a Settings is needed");
		settings.title = "";

		check("a list from a slice, and a record that may be missing",
			Shapes.rightOf([{x: 1, y: 0}, {x: 5, y: 9}], 2).y == 9 && Shapes.rightOf([], 0) == null);
		check("a list of numbers", Shapes.total([1, 2, 3, 4]) == 10 && Shapes.total([]) == 0);
		check("a number that may be missing", Shapes.doubled(21) == 42 && Shapes.doubled(null) == null);
		check("a list of text out", Shapes.words(" one  two ☃ ").join("|") == "one|two|☃" && Shapes.words("").length == 0);
		settings.count = 5;
		var counter = Shapes.counter(settings);
		check("data in and an object out", counter.add(1) == 6);
		counter.close();
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

	// Atomics both sides read and write directly, without a call.
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
		// A thread the library starts, running while no Haxe call is in
		// progress.
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

	// A shared value held by the library and by several Haxe objects at once.
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
		var pots = other.pair();
		check("a list of shared objects", pots.length == 2 && pots[1].total() == 2 * other.total() && Pot.live() == 3);
		for (each in pots)
			each.close();
		check("each released with its holder", Pot.live() == 1);
		weak.close();
		throws("a closed weak reference", () -> weak.upgrade(), "closed");

		a.close();
		check("a share outlives the object that gave it", other.total() == 17 && Pot.live() == 1);
		other.close();
		check("every pot was released", Pot.live() == 0);
	}

	// Haxe functions passed as callbacks, borrowed for the duration of the
	// call.
	static function callbacks() {
		var wide = [haxe.Int64.make(0x12345678, 0x76543210), haxe.Int64.make(0x7fffffff, -1), haxe.Int64.make(-2147483648, 0),
			haxe.Int64.make(-1, -1), haxe.Int64.make(0, 0)];
		for (value in wide) {
			check('all 64 bits reach a callback and return: $value', Numbers.mapWide(value, n -> n) == value);
			check('a callback computes on all 64 bits: $value', Numbers.mapWide(value, n -> ~n) == ~value);
			check('a callback gives all 64 bits: $value', Numbers.wideFrom(() -> value) == value);
			check('text and 64 bits share a callback: $value', Numbers.describeWide(value, (text, n) -> '$text:$n') == 'wide:$value');
		}
		var wideError = new haxe.Exception("wide callback failed");
		var wideCaught:Null<haxe.Exception> = null;
		try {
			Numbers.mapWide(wide[0], _ -> throw wideError);
		} catch (error:haxe.Exception) {
			wideCaught = error;
		}
		check("a 64-bit callback preserves the exception", wideCaught == wideError);
		throws("a callback giving 64 bits can throw", () -> Numbers.wideFrom(() -> throw wideError), "wide callback failed");
		var afterWide = Numbers.mapWide(wide[0], n -> n);
		check('and leaves no failure for the next call: $afterWide expected ${wide[0]}', afterWide == wide[0]);

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

		check("text goes to a lent function and comes back", counter.described((name, value) -> '$name=$value') == "[naïve ☃ 𝄞=48]");
		var words = [];
		check("a word at a time", Counter.words("one  two ☃ three", word -> {
			words.push(word);
			return word.charAt(0) == "t";
		}) == 2 && words.join("|") == "one|two|☃|three");
		check("text from a function that takes nothing", Counter.titled(() -> "𝄞 clef") == 6 && Counter.titled(() -> "") == 0);
		throws("one that gives text can throw", () -> counter.described((_, _) -> throw new haxe.Exception("no name")), "no name");
		throws("and cannot give null", () -> Counter.titled(() -> null), "returned null");

		var chunks = [];
		check("bytes go to a lent function", counter.chunks(3, (bytes, index) -> chunks.push('$index:${bytes.toHex()}')) == 4
			&& chunks.join(" ") == "0:300000 1:00 -1:");
		var live = Counter.live();
		var kept = [];
		check("an object given to a lent function is its to keep", Counter.spawnEach(3, (made, word) -> {
			kept.push(made);
			return made.add(0) * (word == "made" ? 10 : 0);
		}) == 60 && Counter.live() == live + 3);
		check("and to use after the call", kept[2].add(1) == 4);
		for (made in kept)
			made.close();
		throws("one it was given when it threw is not lost", () -> Counter.spawnEach(2, (made, _) -> {
			kept.push(made);
			throw new haxe.Exception("not wanted");
		}), "not wanted");
		check("but is its own to close", kept[3].add(0) == 1 && Counter.live() == live + 1);
		kept[3].close();
		check("or released", Counter.live() == live);
		var pot = new Pot(10);
		var halves = [];
		check("shared objects given to one", Pot.offer(pot, (whole, half) -> {
			halves.push(half);
			var same = whole.same(pot);
			whole.close();
			return same ? half.total() : -1;
		}) == 15 && halves[0].total() == 5 && Pot.live() == 2);
		halves[0].close();
		pot.close();

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

	// Asynchronous results from the library: awaited here, and delivered by
	// the loop.
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

		check("an async function is a future of what it gives", Later.squared(7, 20).await() == 49);
		throws("and of how it fails", () -> Later.squared(100000, 1).await(), "counter overflow");
		check("what it was passed is its own by then", Later.shout("wörld ☃", haxe.io.Bytes.ofHex("010203")).await() == "hello wörld ☃! 3");
		check("one that gives nothing is still waited for", Later.settle(5).await() == hxn.Unit.Nothing && Later.agree(true).await() == hxn.Unit.Nothing);
		throws("or fails", () -> Later.agree(false).await(), "refused later");
		var counted = Later.counted(8).await();
		check("an object made by async work", counted.add(1) == 9);
		var gone = counted.intoValueLater(5);
		throws("an async function that takes the object closes it at once", () -> counted.add(1), "closed");
		check("and the work has it", gone.await() == 9 && Counter.live() == before);
		var found = Later.maybeCounter(5).await();
		check("a future of an object that may be absent", found != null && found.add(0) == 5 && Later.maybeCounter(0).await() == null);
		found.close();
		var one = new Counter(3), two = new Counter(4);
		var summed = Later.totalLater(one, two);
		throws("an async function that takes an object by value closes it at once", () -> one.add(1), "closed");
		check("and the work owns it", summed.await() == 7 && Later.totalLater(new Counter(1), null).await() == 1 && Counter.live() == before);
		var batch = Later.counterBatch(3).await();
		check("a future transfers a batch in order", [for (counter in batch) counter.add(0)].join(",") == "1,2,3"
			&& Counter.live() == before + 3);
		for (counter in batch)
			counter.close();
		check("an empty future batch", Later.counterBatch(0).await().length == 0);
		var asyncBatch = Later.batchLater(2, false).await();
		check("an async function returns a batch", [for (counter in asyncBatch) counter.add(1)].join(",") == "2,3");
		for (counter in asyncBatch)
			counter.close();
		throws("a batch future can fail", () -> Later.batchLater(2, true).await(), "batch refused");
		check("owned batch values are released", Counter.live() == before);
		var potsBefore = Pot.live();
		var sharedBatch = Later.potBatch(7).await();
		check("a future batch preserves shared identity", sharedBatch.length == 2 && sharedBatch[0].same(sharedBatch[1]) && Pot.live() == potsBefore + 1);
		sharedBatch[0].close();
		check("each batch element owns a share", sharedBatch[1].add(1) == 8);
		sharedBatch[1].close();
		var batchSizes = [];
		throws("a stream of batches fails after its batches", () -> {
			for (batch in Later.counterBatches()) {
				batchSizes.push(batch.length);
				check("a stream batch keeps its order", [for (counter in batch) counter.add(0)].join(",") == (batch.length == 0 ? "" : batch.length == 1 ? "1" : "1,2"));
				for (counter in batch)
					counter.close();
			}
		}, "batches ended");
		check("including an empty batch", batchSizes.join(",") == "0,1,2" && Counter.live() == before);
		for (batch in Later.potBatches(9)) {
			check("a stream batch preserves shared identity", batch.length == 2 && batch[0].same(batch[1]));
			for (pot in batch)
				pot.close();
		}
		check("shared batches are released", Pot.live() == potsBefore);

		var kept = new Pot(1);
		var adding = kept.addLater(2, 20);
		kept.close();
		check("an async function of a shared value keeps it alive", adding.await() == 3 && Pot.live() == 0);
		var first = new Pot(1);
		var second = new Pot(10);
		var third = new Pot(100);
		var poured = first.pour(second, null).await();
		check("shared objects passed to async work", poured.total() == 11 && first.pour(second, third).await().total() == 121);
		for (pot in [first, second, third])
			pot.close();
		check("a stream of nothing counts what happened", [for (_ in Later.ticks(4)) 1].length == 4);
		check("a future carries 64 bits", Later.stamp(3).await() == haxe.Int64.make(3, 0));
		var frame = Later.frame(4, 0xAB).await();
		check("and bytes", frame.length == 4 && frame.get(0) == 0xAB && frame.get(3) == 0xAB && Later.frame(0, 1).await().length == 0);
		throws("or the failure of the work that would have made them", () -> Later.frame(-1, 0).await(), "negative size");
		var sizes = [for (frame in Later.frames(5)) frame.length * 10 + frame.get(frame.length - 1)];
		check('a stream of bytes, $sizes', sizes.join(",") == "11,22,33,44,55");

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

		// Delivered when the app calls `Loop.sync`, to all of these in one
		// batch.
		hxn.Loop.manual = true;
		var deliveredBatch = -1;
		Later.counterBatch(2).then(batch -> {
			deliveredBatch = 0;
			for (counter in batch) {
				deliveredBatch += counter.add(0);
				counter.close();
			}
		});
		var listenedBatches = [];
		var batchesFailed = "";
		Later.counterBatches().listen(batch -> {
			listenedBatches.push(batch.length);
			for (counter in batch)
				counter.close();
		}, () -> {}, error -> batchesFailed = error.message);
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
		var odometer = Later.odometer(7);
		var far = haxe.Int64.make(256, 7);
		var waited = 0;
		while ((deliveredBatch < 0 || batchesFailed == "" || squared < 0 || refused == "" || !ended || ramp.peek() != 50 || label.peek() != "3" || odometer.peek() != far) && waited++ < 5000) {
			hxn.Loop.sync();
			Sys.sleep(0.001);
		}
		check("a continuation gets the complete owned batch", deliveredBatch == 3);
		check("a listener gets batches before failure", listenedBatches.join(",") == "0,1,2" && batchesFailed.indexOf("batches ended") >= 0);
		check("delivered batch objects are released", Counter.live() == before);
		check("a continuation is called with the value", squared == 36);
		check('or with the failure: $refused', refused.indexOf("counter overflow") >= 0);
		check('a listener gets every item and the end: $items', items.join(",") == "1,2,3" && ended);
		var rising = true;
		for (i in 1...levels.length)
			if (levels[i] <= levels[i - 1])
				rising = false;
		// A signal reports only its latest value, so the number of
		// intermediate values seen depends on how fast the library ran.
		check('a signal is followed to its last value: $levels', levels[levels.length - 1] == 50 && rising);
		check("and one of text", label.peek() == "3");
		check("and one of 64 bits", odometer.peek() == far);
		stop();
		hxn.Loop.manual = false;
	}

	// Tagged enums, maps, typed errors, and data carried by futures, streams,
	// signals and callbacks.
	static function events() {
		var history = Events.history();
		check("an enum that holds values, one of each case", history.length == 9 && history[0].match(Started));
		check("named fields", switch (history[1]) {
			case Progress(done, total): done == 3 && total == 8;
			case _: false;
		});
		check("text", history[2].match(Message("naïve ☃")));
		check("a record and an enum", switch (history[3]) {
			case Moved(point, mode): point.x == 1.5 && point.y == -2 && mode == Mode.Loud;
			case _: false;
		});
		check("a list of its own kind", history[4].match(Batch([Started, Message("in")])));
		check("a box", history[5].match(Wrapped(Progress(1, 1))));
		check("maps", switch (history[6]) {
			case Labelled(labels, order): labels.get("a") == 1 && Lambda.count(labels) == 1 && order.get(1) == "one" && order.get(2) == "two";
			case _: false;
		});
		check("a box that may be missing", history[7].match(Maybe(null)) && history[8].match(Maybe(Started)));
		var depths = [for (event in history) Events.depth(event)].join(",");
		check('each goes back as it came: $depths', depths == "0,0,0,0,1,1,0,0,1");
		var shown = Events.show(Labelled(["x" => 7], [3 => "three"]));
		check('and is read as it was written: $shown', shown == 'Labelled { labels: {"x": 7}, order: {3: "three"} }');
		var deep = Events.wrapped(Message("in"), 3);
		check("an enum taken and given back", deep.match(Wrapped(Wrapped(Wrapped(Message("in"))))) && Events.depth(deep) == 3);
		#if !emscripten
		// An Emscripten build's stack is too small to serialize a value nested
		// that deep.
		throws("one that nests too deep is refused", () -> Events.depth(Events.wrapped(Started, 200)), "nests too deep");
		#end
		throws("and one that is missing", () -> Events.depth(null), "null where a Event is needed");
		var inverted = Events.inverted(["b" => 2, "a" => 1]);
		check("a map in and a map out", inverted.get(1) == "a" && inverted.get(2) == "b" && Lambda.count(inverted) == 2);
		var none:Map<String, Int> = [];
		check("an empty map", Lambda.count(Events.inverted(none)) == 0);

		check("a function that fails with data returns as any other", Events.checked(2) == 2 && Events.modeNamed("loud") == Mode.Loud);
		var caught:CountException = null;
		try {
			Events.checked(9);
		} catch (error:CountException) {
			caught = error;
		}
		check("a failure with data is an exception that holds it", caught != null && caught.message == "9 is more than 3" && switch (caught.error) {
			case TooMany(limit, given): limit == 3 && given == 9;
			case _: false;
		});
		caught = null;
		try {
			Events.checked(0);
		} catch (error:CountException) {
			caught = error;
		}
		check("each case of it", caught != null && caught.error.match(Empty) && caught.message == "nothing to count");
		caught = null;
		try {
			Events.modeNamed("shrill");
		} catch (error:CountException) {
			caught = error;
		}
		check("with what the case holds", caught != null && caught.error.match(Unknown("shrill")));
		throws("and is still an exception with the library's message", () -> Events.checked(0), "nothing to count");
		Events.refuse(0);
		var refusal:RefusalException = null;
		try {
			Events.refuse(7);
		} catch (error:RefusalException) {
			refusal = error;
		}
		check("an error that is a record",
			refusal != null && refusal.error.code == 7 && refusal.error.reason == "not today ☃" && refusal.message == "refused (7): not today ☃");
		var trouble:TroubleException = null;
		try {
			Events.trouble(5);
		} catch (error:TroubleException) {
			trouble = error;
		}
		check("and one that is a name", trouble != null && trouble.error == Trouble.Major && trouble.message == "major trouble" && Events.trouble(2) == 2);
		throws("a failure of another kind is thrown as it always was", () -> Shapes.preset(Mode.Off), "nothing is preset for off");

		var place = Events.checkedLater(2).await();
		check("a future carries a record", place.x == 2 && place.y == -2);
		var later:CountException = null;
		try {
			Events.checkedLater(5).await();
		} catch (error:CountException) {
			later = error;
		}
		check("and fails with the library's own error", later != null && later.error.match(TooMany(3, 5)) && later.message == "5 is more than 3");
		check("an enum", Events.modeLater(Mode.Off).await() == Mode.Quiet);
		check("a narrow number", Events.smallLater(200).await() == 200);
		var path = Events.pathLater(3).await();
		check("a list of records", path.length == 3 && path[2].x == 2 && Events.pathLater(0).await().length == 0);
		check("a number that may be missing", Events.maybeLater(4).await() == 4 && Events.maybeLater(-1).await() == null);
		var fed = [for (event in Events.feed(5)) Events.depth(event)].join(",");
		check('a stream carries enums that hold values: $fed', fed == "0,0,0,0,1");

		hxn.Loop.manual = true;
		var mode = Events.modeNow();
		var moving = Events.placeNow(4);
		var failed:CountException = null;
		Events.checkedLater(0).then(_ -> {}, error -> failed = Std.downcast(error, CountException));
		var waited = 0;
		while ((mode.peek() != Mode.Off || moving.peek().x != 4 || failed == null) && waited++ < 5000) {
			hxn.Loop.sync();
			Sys.sleep(0.001);
		}
		check("a signal carries an enum", mode.peek() == Mode.Off);
		check("and a record", moving.peek().x == 4 && moving.peek().y == 1);
		check("a continuation is given the library's own error", failed != null && failed.error.match(Empty));
		hxn.Loop.manual = false;

		var modes = Events.eachPoint([{x: 1, y: 0}, {x: -1, y: 0}], point -> point.x > 0 ? Mode.Loud : Mode.Quiet);
		check("a lent function takes a record and gives an enum", modes.length == 2 && modes[0] == Mode.Loud && modes[1] == Mode.Quiet);
		var reshaped = Events.reshaped(Progress(1, 2), event -> Batch([event, Started]));
		check("takes and gives an enum that holds values", reshaped.match(Batch([Progress(1, 2), Started])));
		var picked = Events.picked([{x: 1, y: 1}, {x: 2, y: 2}], (points, mode) -> mode == Mode.Loud ? points[points.length - 1] : null);
		check("takes a list and gives what may be missing",
			picked.x == 2 && Events.picked([], (points, _) -> points.length == 0 ? null : points[0]) == null);
		check("gives bytes", Events.stamped(n -> haxe.io.Bytes.ofHex(n == 1 ? "0a0b" : "")).toHex() == "0a0b");
		check("an enum each way", Events.graded(mode -> mode == Mode.Quiet ? Mode.Off : Mode.Loud) == Mode.Off);
		throws("what one throws reaches the caller", () -> Events.reshaped(Started, _ -> throw new haxe.Exception("no shape")), "no shape");
		throws("and so does a null where it gives data", () -> Events.reshaped(Started, _ -> null), "null where a Event is needed");
		throws("or bytes", () -> Events.stamped(_ -> null), "returned null");
	}

	// Out of line, like `churn`. Returns the peak memory held.
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

	// Native memory the collector cannot see, which the library reports to
	// it. Returns a description of the results for the summary line.
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

		// Dropped without `close`, and with no explicit collection.
		hxn.Gc.budget = 8 * mib;
		var peak = pile(400, mib);
		var left = Blob.live();
		check('dropped values are collected as they pile up: $left of 400 left, ${peak / mib} MiB held at most', peak <= 16 * mib && left <= 16);

		// Under the budget, so these are collected only because the library
		// requests a collection.
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
		var stamped = haxe.io.Bytes.alloc(3);
		counter.stamp(stamped);
		check("bytes written in place", stamped.toHex() == "2a2a2a");
		counter.stamp(haxe.io.Bytes.alloc(0));
		var into = haxe.io.Bytes.alloc(2);
		check("bytes copied into others", Tools.copy(haxe.io.Bytes.ofHex("0102ff"), into) == 2 && into.toHex() == "0102");
		throws("the same bytes read and written", () -> Tools.copy(into, into), "passed as two arguments");
		var parts = counter.split(3);
		check("a list of objects", parts.length == 3 && parts[0].add(0) == 42 && parts[2].add(0) == 44);
		check("an empty list", counter.split(0).length == 0);
		for (part in parts)
			part.close();
		var host = new Counter(1), eaten = new Counter(5);
		check("an object taken by value", host.consume(eaten) == 6);
		throws("closes the object passed", () -> eaten.add(0), "closed");
		throws("which cannot also be the receiver", () -> host.consume(host), "in use");
		host.close();
		throws("a failure in place of bytes", () -> counter.fill(-1), "negative count");
		check("a bool out", counter.isPositive());
		check("a float in and out", counter.scaled(0.5) == 21);
		counter.set(5, true);
		check("a bool in and nothing out", counter.add(0) == -5 && !counter.isPositive());

		throws("a failure is thrown", () -> counter.add(-2147483647), "counter overflow");
		check("and leaves the value usable", counter.add(0) == -5);
		throws("a panic is thrown", Counter.fail, "failed on request");
		Counter.nap(1);

		check("a static function", Counter.live() == 1);
		counter.close();
		check("close drops the value", Counter.live() == 0);
		counter.close();
		throws("use after close", () -> counter.add(1), "closed");

		handles();
		functions();
		numbers();
		data();
		atomics();
		shared();
		callbacks();
		later();
		events();
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
		hxn.Loop.exit(failed == 0 ? 0 : 1);
	}
}
