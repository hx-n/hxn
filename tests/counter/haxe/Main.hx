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

	// Counters whose kept function refers back to them.
	static function cycles(count:Int) {
		for (i in 0...count) {
			var counter = new Counter(i);
			counter.setStep(value -> value + counter.add(0));
		}
	}

	static function main() {
		values();
		Signals.run(check);
		futures();
		frames();
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
		#if !wasm
		// A wasm library is built to abort on a panic, which ends the program.
		throws("a panic is thrown", Counter.fail, "failed on request");
		#end

		check("one counter alive", Counter.live() == 1);
		counter.close();
		check("close drops it", Counter.live() == 0);
		counter.close();
		throws("use after close", () -> counter.add(1), "closed");

		Counter.nap(1);

		var applied = new Counter(1);
		check("a function lent to the library is called with each value", applied.apply(10, value -> value * 2) == 1024);
		var calls = 0;
		throws("what it throws reaches the caller", () -> applied.apply(5, value -> {
			calls++;
			if (calls == 3)
				throw "on the third call";
			return value + 1;
		}), "on the third call");
		check('and stops the library there, after $calls calls', calls == 3 && applied.add(0) == 1026);
		throws("it cannot use the counter that is calling it", () -> applied.apply(1, value -> applied.add(1)), "in use");
		check("which is free again afterwards", applied.add(0) == 1026);
		applied.close();

		var advancing = new Counter(1);
		throws("a kept function has to be set first", () -> advancing.advance(1), "no step set");
		advancing.setStep(value -> value * 3);
		check("a function the binding keeps is lent to each later call", advancing.advance(2) == 9 && advancing.advance(1) == 27);
		advancing.close();
	}

	static function futures() {
		var square = Counter.squareLater(7, 100);
		#if !wasm
		// Without threads the work has already run by the time the call returns.
		check("a future starts pending", !square.isReady());
		#end
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

		// Work done in steps: on its own thread, or by this one as it waits.
		var sum = Counter.sumLater(20000);
		#if wasm
		check("stepped work is still pending when the call returns", !sum.isReady());
		#end
		check("and finishes while it is awaited", sum.await() == 200010000);
		check("work that finishes in its first step is ready at once", Counter.sumLater(0).isReady());
		throws("a step can fail", () -> Counter.sumLater(70000).await(), "counter overflow");
		var sums = [for (i in 0...20) Counter.sumLater(1000 + i)];
		var all = 0;
		for (future in sums)
			all += future.await() - 500500;
		check('stepped work interleaves: $all', all == 191330);

		// Async work, built on the two kinds above.
		check("async work awaits other futures", Counter.combinedLater(7, 100).await() == 5099);
		throws("and passes their failures on", () -> Counter.combinedLater(100000, 1).await(), "counter overflow");
		var combined = [for (i in 0...50) Counter.combinedLater(i, 10)];
		var sumOfAll = 0;
		for (future in combined)
			sumOfAll += future.await();
		check('many async at once: $sumOfAll', sumOfAll == 43175);

		// Streams: results that arrive more than once, read like an iterator.
		check("a stream is read item by item", [for (item in Counter.countUp(5)) item].join(",") == "1,2,3,4,5");
		check("one with nothing in it just ends", [for (item in Counter.countUp(0)) item].length == 0);
		var read = 0;
		throws("a stream's failure is thrown in its turn", () -> {
			for (item in Counter.countUp(3, true))
				read += item;
		}, "counter overflow");
		check('after the items before it: $read', read == 6);

		#if !emscripten
		var emitter = new Counter.Emitter();
		var emitted = emitter.stream();
		sys.thread.Thread.create(() -> {
			for (i in 0...3) {
				Sys.sleep(0.005);
				emitter.emit(i * 10);
			}
			emitter.end();
		});
		check("items another thread sends arrive in order", [for (item in emitted) item].join(",") == "0,10,20");
		check("and an ended stream takes no more", !emitter.emit(1));
		#end

		#if !emscripten
		// Settled from Haxe, after the call that made the future has returned.
		// hxcpp under Emscripten has no second thread to settle it from.
		var promise = new Counter.Promise();
		var later = promise.future();
		check("a promised future is pending", !later.isReady());
		sys.thread.Thread.create(() -> {
			Sys.sleep(0.02);
			check("the first settle wins", promise.resolve(7));
			check("and a second is refused", !promise.resolve(8));
		});
		check("await sees what another thread settled", later.await() == 7);
		#end
	}

	// An app with a loop of its own: nothing is delivered until it asks,
	// and then everything is, in the order the library produced it.
	static function frames() {
		hxn.Loop.manual = true;
		var log = [];
		var first = new Counter.Promise();
		var second = new Counter.Promise();
		var emitter = new Counter.Emitter();
		first.future().then(value -> log.push('first $value'));
		second.future().then(value -> log.push('second $value'));
		emitter.stream().listen(item -> log.push('item $item'), () -> log.push("end"));
		hxn.Loop.sync();
		first.resolve(1);
		emitter.emit(2);
		second.resolve(3);
		emitter.end();
		check('nothing is delivered until the app asks: $log', log.length == 0);
		hxn.Loop.sync();
		check('then everything is, in the order it happened: $log', log.join(",") == "first 1,item 2,end,second 3");

		// A frame loop waiting on work the library does in steps.
		var sum = -1;
		Counter.sumLater(20000).then(value -> sum = value);
		var turns = 0;
		while (sum < 0 && turns++ < 20000) {
			hxn.Loop.sync();
			Sys.sleep(0.001);
		}
		check('a frame loop is all it takes to finish stepped work: $sum', sum == 200010000);

		// Two values set together are one rerun, and a set made through the
		// binding is seen at once, not a delivery later.
		var width = new Counter.Gauge(2);
		var height = new Counter.Gauge(3);
		var areas = [];
		var dispose = hxn.Reactive.root(dispose -> {
			var w = width.signal(), h = height.signal();
			hxn.Reactive.effect(() -> areas.push(w.value * h.value));
			return dispose;
		});
		width.set(4);
		check('a set through the binding is seen at once: $areas', areas.join(",") == "6,12");
		hxn.Reactive.batch(() -> {
			width.set(10);
			height.set(10);
		});
		hxn.Loop.sync();
		check('values set together are one rerun, and delivery adds none: $areas', areas.join(",") == "6,12,100");
		dispose();
		check("nothing is left waiting", !hxn.Loop.sync());
		hxn.Loop.manual = false;
	}

	// Delivered by this thread's event loop, which runs once `main` returns.
	static function continuations() {
		Counter.squareLater(9, 10).then(value -> {
			check("then delivers the value", value == 81);
			check("from the event loop", returned);
			Counter.squareLater(100000, 1).then(_ -> check("then took a rejection for a value", false), error -> {
				check('then delivers the rejection: ${error.message}', error.message.indexOf("counter overflow") >= 0);
				settledLater();
			});
		});
	}

	// Still pending when `then` is called, and settled by a later turn of the loop.
	static function settledLater() {
		var promise = new Counter.Promise();
		var future = promise.future();
		future.then(value -> {
			check("then delivers a later settle", value == 7);
			Counter.sumLater(5000).then(sum -> {
				check("then delivers stepped work", sum == 12502500);
				wokenLater();
			});
		});
		check("which had not happened yet", !future.isReady());
		sys.thread.Thread.current().events.run(() -> check("the loop settles it", promise.resolve(7)));
	}

	// Async work waiting on something the loop settles later: it is woken,
	// not asked over and over.
	static function wokenLater() {
		var promise = new Counter.Promise();
		var doubled = promise.doubled();
		doubled.then(value -> {
			check("async work is woken by a later settle", value == 14);
			listened();
		});
		check("and was waiting until then", !doubled.isReady());
		sys.thread.Thread.current().events.run(() -> check("the loop settles what it awaits", promise.resolve(7)));
	}

	// Items sent by later turns of the loop, each handed to the listener.
	static function listened() {
		var emitter = new Counter.Emitter();
		var got = [];
		emitter.stream().listen(item -> got.push(item), () -> {
			check('listen delivers each item and then the end: $got', got.join(",") == "1,2,3");
			listenedToFailure();
		});
		var loop = sys.thread.Thread.current().events;
		loop.run(() -> {
			emitter.emit(1);
			emitter.emit(2);
			loop.run(() -> {
				emitter.emit(3);
				emitter.end();
			});
		});
	}

	static function listenedToFailure() {
		var got = [];
		Counter.countUp(2, true).listen(item -> got.push(item), () -> check("a failed stream does not also end", false), error -> {
			check('listen delivers a failure after the items: $got', got.join(",") == "1,2" && error.message.indexOf("counter overflow") >= 0);
			followed();
		});
	}

	// A signal that follows a value in the library, with a computed value
	// and an effect depending on it.
	static function followed() {
		var loop = sys.thread.Thread.current().events;
		var gauge = new Counter.Gauge(5);
		var seen = [];
		var dispose = hxn.Reactive.root(dispose -> {
			var level = gauge.signal();
			var doubled = new hxn.Computed(() -> level.value * 2);
			hxn.Reactive.effect(() -> {
				seen.push(doubled.value);
				if (doubled.value == 14)
					loop.run(() -> {
						check('effects follow a value the library holds: $seen', seen.join(",") == "10,12,14");
						dispose();
						check("and stop when their root is disposed", gauge.set(99));
						ramped();
					});
			});
			return dispose;
		});
		check("a followed signal starts at the library's value", seen.join(",") == "10");
		gauge.set(6);
		// This runs inside a delivery, which is one batch: the signal has
		// the value at once and effects rerun when the delivery ends.
		check('and has a value set here at once: $seen', gauge.signal().peek() == 6 && seen.join(",") == "10");
		loop.run(() -> gauge.set(7));
	}

	// A value the library changes by itself, faster than the loop turns.
	static function ramped() {
		var loop = sys.thread.Thread.current().events;
		var values = [];
		hxn.Reactive.root(dispose -> {
			var ramp = Counter.ramp(2000);
			hxn.Reactive.effect(() -> {
				values.push(ramp.value);
				if (ramp.value == 2000)
					loop.run(() -> {
						var rising = true;
						for (i in 1...values.length)
							if (values[i] <= values[i - 1])
								rising = false;
						check('a fast value is followed at its latest, ${values.length} of 2000 seen', rising && values.length <= 2001);
						dispose();
						finish();
					});
			});
		});
	}

	static function finish() {
		#if emscripten
		// hxcpp's collector does not return from a forced collection under
		// Emscripten, so nothing here can show a finalizer running.
		check("and hxn reports that it cannot run one", !hxn.Gc.collect());
		var collected = "no collection was run";
		#else
		churn(500);
		cycles(500);
		check("a collection is run", hxn.Gc.collect());
		var left = Counter.live();
		check('the collector releases the rest, $left left', left < 50);
		var collected = '$left of 1000 unclosed counters left after a collection, 500 of them referred to by their own kept function';
		#end

		Sys.println(failed == 0 ? 'PASS ($collected)' : 'FAILED $failed');
		hxn.Loop.exit(failed == 0 ? 0 : 1);
	}
}
