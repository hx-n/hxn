package hxn;

import haxe.Exception;
import sys.thread.Mutex;
import sys.thread.Thread;

/**
	One result or failure from an asynchronous native operation.

	`await()` returns the value or throws the failure, blocking the calling
	thread while letting the collector run. `then()` delivers either one later,
	on the thread that asked.

	A binding builds one from three calls into its library: the state
	(0 pending, 1 resolved, 2 rejected), a wait taking a limit in milliseconds
	(negative for none) and returning the state, and a take that returns the
	value or throws. The result stays in the library until `take` runs here, so
	the native thread that settles the future never touches this runtime.
**/
class Future<T> {
	final state:() -> Int;
	final wait:Int->Int;
	final take:() -> T;
	final lock = new Mutex();
	var taken = false;
	var value:Null<T>;
	var failure:Null<Exception>;

	public function new(state:() -> Int, wait:Int->Int, take:() -> T) {
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
		wait(-1);
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
		Calls `onValue` or `onError` once the future is settled, from the event
		loop of the thread that called `then`. The main thread runs its loop
		when `main` returns; a thread without one cannot call this.

		A failure with no `onError` is thrown from that event loop.
	**/
	public function then(onValue:T->Void, ?onError:Exception->Void):Void {
		var events = Thread.current().events;
		events.promise();
		Thread.create(() -> {
			var result:Null<T> = null;
			var error:Null<Exception> = null;
			try
				result = await()
			catch (e:Exception)
				error = e;
			events.runPromised(() -> {
				if (error == null)
					onValue(result);
				else if (onError != null)
					onError(error);
				else
					throw error;
			});
		});
	}
}
