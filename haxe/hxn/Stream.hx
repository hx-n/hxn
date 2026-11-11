package hxn;

import haxe.Exception;

/**
	Results from a native library that arrive more than once, such as events.

	It reads like any iterator: `for (item in stream)` waits for each item and
	stops when the stream ends, and a failure that ended it is thrown in its
	turn. `listen` delivers the same later, with the library's other results.

	The library never calls a Haxe function to report an item. It queues, and
	a Haxe thread takes each item and calls what it likes with it, so the
	native thread that produced the item never touches this runtime.

	A binding builds one from its library's `Inbox` and three calls: one that
	starts the stream being noted there and returns its id, a wait taking a
	limit in milliseconds (negative for none) and returning 0 while there is
	nothing yet, 1 when there is something to take and 2 once the stream has
	ended, and a take that returns the next item or throws. One reader at a time.
**/
class Stream<T> {
	final inbox:Inbox;
	final watch:() -> Int;
	final wait:Int->Int;
	final take:() -> T;

	public function new(inbox:Inbox, watch:() -> Int, wait:Int->Int, take:() -> T) {
		this.inbox = inbox;
		this.watch = watch;
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
		Calls `onItem` with each item and `onEnd` when the stream ends, when
		the library's results are next delivered: see `Loop`. A failure that
		ends it goes to `onError` instead, or is thrown from there if there is
		none.
	**/
	public function listen(onItem:T->Void, ?onEnd:() -> Void, ?onError:Exception->Void):Void {
		inbox.watch(watch(), () -> {
			var flow = wait(0);
			while (flow == 1) {
				var item:T;
				try {
					item = take();
				} catch (error:Exception) {
					if (onError == null)
						throw error;
					onError(error);
					return false;
				}
				onItem(item);
				flow = wait(0);
			}
			if (flow == 0)
				return true;
			if (onEnd != null)
				onEnd();
			return false;
		});
	}
}
