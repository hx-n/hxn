package hxn;

import haxe.Exception;
import sys.thread.Mutex;
import sys.thread.Thread;

/**
	Delivers one native library's results to the app.

	The library has a single queue. It adds an entry for each watched future,
	stream or value that has something new: a settled future, a stream with
	an item, a changed value. This class drains that queue and calls the
	watchers registered for each entry. All results from a library are
	therefore delivered through one path, in order, and one wait covers all
	of them.

	A binding creates one for its library from two functions: `wait` takes a
	timeout in milliseconds (negative for no timeout) and returns whether
	anything is queued, and `next` returns the id of the next queued entry,
	or -1.

	`Future.then`, `Stream.listen` and `Reactive.follow` are built on `watch`.
	An app with its own reactive graph can call `follow` here directly.
**/
class Inbox {
	final wait:Int->Bool;
	final next:() -> Int;
	final lock = new Mutex();
	final watchers = new Map<Int, Array<() -> Bool>>();
	var watching = 0;
	// Ids watched since the last sync. Each gets an initial delivery regardless of what is queued.
	var due:Array<Int> = [];
	var driven = false;
	var looking = false;

	public function new(wait:Int->Bool, next:() -> Int) {
		this.wait = wait;
		this.next = next;
		Loop.inboxes.push(this);
	}

	/**
		Registers `deliver` to be called during `sync` whenever the library
		queues `id`, and once initially. `deliver` returns whether it should
		keep being called. Returns a function that unregisters it.
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
		Calls `sink` with a value the library keeps up to date, each time it
		has changed since `seen`, the version at which the caller read it.
		`wait` and `read` are the library's functions for that value, as
		described in `Reactive.follow`. Returns a function that stops it.
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
		Delivers this library's ready results on the calling thread. Returns
		whether anything is still being watched. `Loop.sync` calls this for
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
					// An exception from one watcher does not stop the others from being called.
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
		// The delivery may have dropped the last reference to a large native value.
		Gc.check();
		if (failure != null)
			throw failure;
		return watching > 0;
	}

	// Arranges for `sync` to be called while anything is watched, unless the app calls it itself.
	function drive():Void {
		if (Loop.manual)
			return;
		var events = Thread.current().events;
		#if (emscripten || hxn_no_threads)
		// No thread can block waiting on the library, so each event loop iteration polls it.
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
		// Schedules a sync so a new watcher's initial delivery does not wait for the library to queue something.
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
		// Keeps the event loop alive while anything is watched.
		events.promise();
		var handled = new sys.thread.Lock();
		Thread.create(() -> {
			while (!Loop.manual && watching > 0) {
				// Waits in short slices with a sleep between them, for
				// runtimes that run their other threads only while this one
				// sleeps.
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
				// Blocks until this delivery has run, so the library is
				// polled again only after the app has handled it.
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
