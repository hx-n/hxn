package hxn;

import haxe.Exception;
import sys.thread.Mutex;
import sys.thread.Thread;

/**
	One native library's results on their way to the app.

	The library notes each thing that has something new, a settled future, a
	stream with an item, a changed value, in one queue. This drains that queue
	and calls whoever is watching each one, so everything from a library
	arrives through one door, in order, and with one wait between them all.

	A binding makes one for its library from two calls: a wait taking a limit
	in milliseconds (negative for none) that returns whether anything is
	queued, and a next that returns the id of what is, or -1.

	`Future.then`, `Stream.listen` and `Reactive.follow` are built on `watch`.
	An app with a reactive graph of its own can use `follow` here directly.
**/
class Inbox {
	final wait:Int->Bool;
	final next:() -> Int;
	final lock = new Mutex();
	final watchers = new Map<Int, Array<() -> Bool>>();
	var watching = 0;
	// Watched since the last sync, and owed a first look whatever the queue says.
	var due:Array<Int> = [];
	var driven = false;
	var looking = false;

	public function new(wait:Int->Bool, next:() -> Int) {
		this.wait = wait;
		this.next = next;
		Loop.inboxes.push(this);
	}

	/**
		Has `deliver` called during `sync` whenever the library has something
		new under `id`, and once to begin with. It returns whether to go on
		being called. Returns what stops it.
	**/
	public function watch(id:Int, deliver:() -> Bool):() -> Void {
		lock.acquire();
		var list = watchers.get(id);
		if (list == null)
			watchers.set(id, [deliver]);
		else
			list.push(deliver);
		watching++;
		due.push(id);
		lock.release();
		drive();
		return () -> unwatch(id, deliver);
	}

	/**
		Calls `sink` with a value the library keeps current, each time it has
		changed since `seen`, the version the caller read it at. `wait` and
		`read` are the library's calls for that value, as `Reactive.follow`
		describes them. Returns what stops it.
	**/
	public function follow<T>(id:Int, seen:Int, wait:(seen:Int, limit:Int) -> Int, read:() -> T, sink:T->Void):() -> Void {
		return watch(id, () -> {
			var version = wait(seen, 0);
			if (version == -1)
				return false;
			if (version != seen) {
				seen = version;
				sink(read());
			}
			return true;
		});
	}

	function unwatch(id:Int, deliver:() -> Bool):Void {
		lock.acquire();
		var list = watchers.get(id);
		if (list != null && list.remove(deliver)) {
			watching--;
			if (list.length == 0)
				watchers.remove(id);
		}
		lock.release();
	}

	/**
		Delivers what this library has ready, on the calling thread. Returns
		whether anything is still being watched. `Loop.sync` does this for
		every library.
	**/
	public function sync():Bool {
		lock.acquire();
		var first = due;
		due = [];
		lock.release();
		var failure:Null<Exception> = null;
		var look = (id:Int) -> {
			lock.acquire();
			var list = watchers.get(id);
			var called = list == null ? null : list.copy();
			lock.release();
			if (called != null)
				for (deliver in called) {
					var keep = false;
					// One watcher that throws does not cost the others their turn.
					try {
						keep = deliver();
					} catch (error:Exception) {
						if (failure == null)
							failure = error;
					}
					if (!keep)
						unwatch(id, deliver);
				}
		};
		Reactive.batch(() -> {
			for (id in first)
				look(id);
			wait(0);
			var id = next();
			while (id >= 0) {
				look(id);
				id = next();
			}
		});
		if (failure != null)
			throw failure;
		return watching > 0;
	}

	// Sees that `sync` gets called while anything is watched, unless the app does.
	function drive():Void {
		if (Loop.manual)
			return;
		var events = Thread.current().events;
		#if (emscripten || hxn_no_threads)
		// No thread to wait on the library with, so each turn of the loop asks.
		if (driven)
			return;
		driven = true;
		var turn:Null<() -> Void> = null;
		turn = () -> {
			if (Loop.manual) {
				driven = false;
				return;
			}
			var more = false;
			try {
				more = sync();
			} catch (error:Exception) {
				events.run(turn);
				throw error;
			}
			if (more)
				events.run(turn);
			else
				driven = false;
		};
		events.run(turn);
		#else
		// A new watcher's first look need not wait for the library to stir.
		if (!looking) {
			looking = true;
			events.run(() -> {
				looking = false;
				if (!Loop.manual)
					sync();
			});
		}
		if (driven)
			return;
		driven = true;
		// Kept alive while anything is watched.
		events.promise();
		var handled = new sys.thread.Lock();
		Thread.create(() -> {
			while (!Loop.manual && watching > 0) {
				// In slices with a sleep between, for runtimes that run their
				// other threads only while this one sleeps.
				if (!wait(20)) {
					Sys.sleep(0.001);
					continue;
				}
				events.promise();
				events.runPromised(() -> {
					var failure:Null<Exception> = null;
					if (!Loop.manual)
						try {
							sync();
						} catch (error:Exception) {
							failure = error;
						}
					handled.release();
					if (failure != null)
						throw failure;
				});
				// One delivery at a time, so the library is asked again only
				// once the app has caught up.
				handled.wait();
			}
			events.runPromised(() -> {
				driven = false;
				if (watching > 0)
					drive();
			});
		});
		#end
	}
}
