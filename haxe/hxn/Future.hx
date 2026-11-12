package hxn;

import haxe.Exception;
import sys.thread.Mutex;

/**
	The single result or failure of an asynchronous native operation.

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
**/
class Future<T> {
	final inbox:Inbox;
	final watch:() -> Int;
	final state:() -> Int;
	final wait:Int->Int;
	final take:() -> T;
	final lock = new Mutex();
	var taken = false;
	var value:Null<T>;
	var failure:Null<Exception>;

	public function new(inbox:Inbox, watch:() -> Int, state:() -> Int, wait:Int->Int, take:() -> T) {
		this.inbox = inbox;
		this.watch = watch;
		this.state = state;
		this.wait = wait;
		this.take = take;
	}

	/** True once the future has a value or a failure. **/
	public function isReady():Bool
		return taken || state() != 0;

	public function isRejected():Bool
		return taken ? failure != null : state() == 2;

	/** Waits for the result. Any thread may call it, any number of times. **/
	public function await():T {
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
		if (failure != null)
			throw failure;
		return value;
	}

	/**
		Calls `onValue` or `onError` after the future has settled, the next
		time the library's results are delivered. That happens on the event
		loop of the thread that first requested a delivery from this library,
		or in `Loop.sync` when the app drives deliveries itself.

		If the future failed and there is no `onError`, the failure is thrown
		from that delivery.
	**/
	public function then(onValue:T->Void, ?onError:Exception->Void):Void {
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
}
