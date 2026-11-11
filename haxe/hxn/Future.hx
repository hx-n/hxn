package hxn;

import haxe.Exception;
import sys.thread.Mutex;

/**
	One result or failure from an asynchronous native operation.

	`await()` returns the value or throws the failure, blocking the calling
	thread while letting the collector run. `then()` delivers either one later,
	when the library's results are next delivered: see `Loop`.

	A binding builds one from its library's `Inbox` and four calls: one that
	starts the future being noted there and returns its id, the state
	(0 pending, 1 resolved, 2 rejected), a wait taking a limit in milliseconds
	(negative for none) and returning the state, and a take that returns the
	value or throws. The result stays in the library until `take` runs here, so
	the native thread that settles the future never touches this runtime.
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

	/** True once there is a value or a failure. **/
	public function isReady():Bool
		return taken || state() != 0;

	public function isRejected():Bool
		return taken ? failure != null : state() == 2;

	/** Waits for the result. Any thread may call it, any number of times. **/
	public function await():T {
		// In slices, with a sleep between them. Some runtimes run their other
		// threads only while this one sleeps, and one without threads cannot
		// block in the library at all: its wait returns while still pending.
		while (wait(20) == 0)
			Sys.sleep(0.001);
		// The library gives the result up once; every caller reads this copy.
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
		Calls `onValue` or `onError` once the future is settled, when the
		library's results are next delivered: from the event loop of the thread
		that first asked this library for one, or from `Loop.sync` in an app
		that drives deliveries itself.

		A failure with no `onError` is thrown from there.
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
