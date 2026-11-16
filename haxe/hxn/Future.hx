package hxn;

import haxe.Exception;
import sys.thread.Mutex;

/**
	The single result or failure of an asynchronous operation.

	`await()` returns the value or throws the failure. It blocks the calling
	thread but lets the garbage collector run. `then()` delivers the value or
	failure later, the next time the library's results are delivered: see
	`Loop`.

	A binding constructs one from its library's `Inbox` and four functions:
	`watch` registers the future with the library's queue and returns its id,
	`state` returns the state (0 pending, 1 resolved, 2 rejected), `wait`
	takes a timeout in milliseconds (negative for no timeout) and returns the
	state, and `take` returns the value or throws. The result stays in the
	library until `take` runs on a Haxe thread, so the native thread that
	settles the future never calls into this runtime.

	`map`, `flatMap` and `recover` derive a future from another, and
	`resolved` and `rejected` create settled ones. A derived future settles
	during its source's delivery, and `await` on it waits for the source.
**/
class Future<T> {
	// A native future's library functions; null for a future made in Haxe.
	final inbox:Null<Inbox>;
	final watch:Null<() -> Int>;
	final state:Null<() -> Int>;
	final wait:Null<Int->Int>;
	final take:Null<() -> T>;
	final lock = new Mutex();
	var taken = false;
	var value:Null<T>;
	var failure:Null<Exception>;
	// For a derived future: blocks until its source has settled it. `await` calls it.
	var block:Null<() -> Void>;
	// For a future made in Haxe: the callbacks `then` registered before it settled.
	var waiting:Null<Array<{onValue:T->Void, onError:Null<Exception->Void>}>>;

	public function new(?inbox:Inbox, ?watch:() -> Int, ?state:() -> Int, ?wait:Int->Int, ?take:() -> T) {
		this.inbox = inbox;
		this.watch = watch;
		this.state = state;
		this.wait = wait;
		this.take = take;
		if (inbox == null)
			waiting = [];
	}

	/** A future that already has `value`. **/
	public static function resolved<T>(value:T):Future<T> {
		var future = new Future<T>();
		future.settle(value, null);
		return future;
	}

	/** A future that already failed with `failure`. **/
	public static function rejected<T>(failure:Exception):Future<T> {
		var future = new Future<T>();
		future.settle(null, failure);
		return future;
	}

	/** True once the future has a value or a failure. **/
	public function isReady():Bool
		return taken || (inbox != null && state() != 0);

	public function isRejected():Bool
		return taken ? failure != null : inbox != null && state() == 2;

	/** Waits for the result. Any thread may call it, any number of times. **/
	public function await():T {
		if (inbox == null) {
			if (!taken && block != null)
				block();
			if (!taken)
				throw new Exception("this future settles only when it is delivered: use then()");
		} else {
			// Waits in short slices with a sleep between them. Some runtimes run
			// their other threads only while this one sleeps. A runtime without
			// threads cannot block in the library at all: its wait returns while
			// the future is still pending.
			while (wait(20) == 0)
				Sys.sleep(0.001);
			// The library hands over the result only once, so it is cached here for every caller.
			lock.acquire();
			if (!taken) {
				try
					value = take()
				catch (e:Exception)
					failure = e;
				taken = true;
			}
			lock.release();
		}
		if (failure != null)
			throw failure;
		return value;
	}

	/**
		Calls `onValue` or `onError` after the future has settled, the next
		time the library's results are delivered. That happens on the event
		loop of the thread that first requested a delivery from this library,
		or in `Loop.sync` when the app drives deliveries itself. On a future
		made in Haxe that has settled, it calls at once.

		If the future failed and there is no `onError`, the failure is thrown
		from that delivery.
	**/
	public function then(onValue:T->Void, ?onError:Exception->Void):Void {
		if (inbox == null) {
			lock.acquire();
			if (!taken) {
				waiting.push({onValue: onValue, onError: onError});
				lock.release();
				return;
			}
			lock.release();
			deliver(onValue, onError);
			return;
		}
		inbox.watch(watch(), () -> {
			if (state() == 0)
				return true;
			var result:Null<T> = null;
			var error:Null<Exception> = null;
			try
				result = await()
			catch (e:Exception)
				error = e;
			if (error == null)
				onValue(result);
			else if (onError != null)
				onError(error);
			else
				throw error;
			return false;
		});
	}

	/**
		A future of `transform` applied to this future's value. A failure passes through
		unchanged.
	**/
	public function map<R>(transform:T->R):Future<R> {
		var next:Future<R> = null;
		// Runs once this future has settled, from its delivery or from an `await` on `next`.
		function complete() {
			if (next.taken)
				return;
			if (failure != null) {
				next.settle(null, failure);
				return;
			}
			// Only `transform` is caught: what `next`'s callbacks throw reaches the caller.
			var result:Null<R> = null;
			var error:Null<Exception> = null;
			try
				result = transform(value)
			catch (e:Exception)
				error = e;
			next.settle(result, error);
		}
		next = derived(() -> {
			waitSettled();
			complete();
		});
		then(_ -> complete(), _ -> complete());
		return next;
	}

	/**
		A future that settles like the future `transform` returns for this future's value.
		A failure passes through unchanged.
	**/
	public function flatMap<R>(transform:T->Future<R>):Future<R> {
		return chain(next -> {
			if (failure != null) {
				next.settle(null, failure);
				return null;
			}
			return try transform(value) catch (e:Exception) rejected(e);
		});
	}

	/**
		A future with this future's value, or, if this one fails, the result of the future
		`transform` returns for the failure.
	**/
	public function recover(transform:Exception->Future<T>):Future<T> {
		return chain(next -> {
			if (failure == null) {
				next.settle(value, null);
				return null;
			}
			return try transform(failure) catch (e:Exception) rejected(e);
		});
	}

	/**
		Calls `start` once this future has settled. `next` takes the result of the future
		`start` returns, or `start` settles `next` itself and returns null.
	**/
	function chain<R>(start:Future<R>->Null<Future<R>>):Future<R> {
		var inner:Null<Future<R>> = null;
		var started = false;
		var next:Future<R> = null;
		function begin() {
			if (started || next.taken)
				return;
			started = true;
			inner = start(next);
			if (inner != null)
				inner.then(r -> next.settle(r, null), e -> next.settle(null, e));
		}
		next = derived(() -> {
			waitSettled();
			begin();
			if (inner != null && !next.taken) {
				inner.waitSettled();
				next.settle(inner.value, inner.failure);
			}
		});
		then(_ -> begin(), _ -> begin());
		return next;
	}

	static function derived<R>(block:() -> Void):Future<R> {
		var future = new Future<R>();
		future.block = block;
		return future;
	}

	// Waits until this future has settled, without throwing its failure.
	function waitSettled():Void {
		if (inbox != null)
			try
				await()
			catch (_:Exception) {}
		else if (!taken && block != null)
			block();
	}

	function settle(v:Null<T>, e:Null<Exception>):Void {
		lock.acquire();
		if (taken) {
			lock.release();
			return;
		}
		taken = true;
		value = v;
		failure = e;
		block = null;
		var callbacks = waiting.splice(0, waiting.length);
		lock.release();
		for (c in callbacks)
			deliver(c.onValue, c.onError);
	}

	function deliver(onValue:T->Void, onError:Null<Exception->Void>):Void {
		if (failure == null)
			onValue(value);
		else if (onError != null)
			onError(failure);
		else
			throw failure;
	}
}
