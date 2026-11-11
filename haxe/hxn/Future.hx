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
		Calls `onValue` or `onError` once the future is settled, from the event
		loop of the thread that called `then`. The main thread runs its loop
		when `main` returns; a thread without one cannot call this.

		A failure with no `onError` is thrown from that event loop.

		Where the runtime has threads a helper thread waits for the result.
		Where it has none (hxcpp under Emscripten, or `-D hxn_no_threads`) the
		event loop asks for the state on every turn instead.
	**/
	public function then(onValue:T->Void, ?onError:Exception->Void):Void {
		var events = Thread.current().events;
		#if (emscripten || hxn_no_threads)
		// Asked again on every turn of the loop, as an event of its own each
		// time: the loop has nothing to sleep on while this is outstanding.
		var poll:Null<() -> Void> = null;
		poll = () -> {
			if (wait(0) == 0)
				events.run(poll);
			else
				deliver(onValue, onError);
		};
		events.run(poll);
		#else
		events.promise();
		Thread.create(() -> {
			// Take the result here, so the event loop only has to hand it over.
			try
				await()
			catch (_:Exception) {}
			events.runPromised(() -> deliver(onValue, onError));
		});
		#end
	}

	function deliver(onValue:T->Void, onError:Null<Exception->Void>):Void {
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
	}
}
