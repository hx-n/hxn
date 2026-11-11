package hxn;

import haxe.Exception;
import sys.thread.Thread;

/**
	Results from a native library that arrive more than once, such as events.

	It reads like any iterator: `for (item in stream)` waits for each item and
	stops when the stream ends, and a failure that ended it is thrown in its
	turn. `listen` delivers the same later, on the thread that asked.

	The library never calls a Haxe function to report an item. It queues, and
	a Haxe thread takes each item and calls what it likes with it, so the
	native thread that produced the item never touches this runtime.

	A binding builds one from two calls into its library: a wait taking a limit
	in milliseconds (negative for none) and returning 0 while there is nothing
	yet, 1 when there is something to take and 2 once the stream has ended, and
	a take that returns the next item or throws. One reader at a time.
**/
class Stream<T> {
	final wait:Int->Int;
	final take:() -> T;

	public function new(wait:Int->Int, take:() -> T) {
		this.wait = wait;
		this.take = take;
	}

	/** Waits for the next item. False once the stream has ended. **/
	public function hasNext():Bool
		return settle() == 1;

	/** The item `hasNext` found, or the failure that ended the stream, thrown. **/
	public function next():T
		return take();

	// In slices, with a sleep between them, as `Future.await` waits.
	function settle():Int {
		var flow = wait(20);
		while (flow == 0) {
			Sys.sleep(0.001);
			flow = wait(20);
		}
		return flow;
	}

	/**
		Calls `onItem` with each item, from the event loop of the thread that
		called `listen`, and `onEnd` when the stream ends. A failure that ends
		it goes to `onError` instead, or is thrown from that loop if there is
		none. A thread without an event loop cannot call this.
	**/
	public function listen(onItem:T->Void, ?onEnd:() -> Void, ?onError:Exception->Void):Void {
		var events = Thread.current().events;
		var ended = () -> {
			if (onEnd != null)
				onEnd();
		};
		var failed = (error:Exception) -> {
			if (onError != null)
				onError(error);
			else
				throw error;
		};
		#if (emscripten || hxn_no_threads)
		// One item a turn of the loop, so a long stream does not hold it.
		var poll:Null<() -> Void> = null;
		poll = () -> {
			switch wait(0) {
				case 0:
					events.run(poll);
				case 1:
					var item:T;
					try {
						item = take();
					} catch (error:Exception) {
						failed(error);
						return;
					}
					events.run(poll);
					onItem(item);
				default:
					ended();
			}
		};
		events.run(poll);
		#else
		// Kept until the stream ends, however many items come first.
		events.promise();
		Thread.create(() -> {
			while (true) {
				if (settle() != 1) {
					events.runPromised(ended);
					return;
				}
				var item:T;
				try {
					item = take();
				} catch (error:Exception) {
					events.runPromised(() -> failed(error));
					return;
				}
				events.promise();
				events.runPromised(() -> onItem(item));
			}
		});
		#end
	}
}
