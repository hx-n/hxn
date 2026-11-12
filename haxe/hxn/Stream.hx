package hxn;

import haxe.Exception;

/**
	A sequence of results from a native library, such as events.

	It is an iterator: `for (item in stream)` blocks for each item and stops
	when the stream ends. If the stream ended with a failure, the failure is
	thrown after the items before it have been read. `listen` delivers the
	same items and failure later, with the library's other results.

	The library never calls a Haxe function to report an item. It queues
	items, and a Haxe thread takes each one and passes it to Haxe code, so
	the native thread that produced the item never calls into this runtime.

	A binding constructs one from its library's `Inbox` and three functions:
	`watch` registers the stream with the library's queue and returns its id,
	`wait` takes a timeout in milliseconds (negative for no timeout) and
	returns 0 while there is nothing yet, 1 when there is something to take
	and 2 once the stream has ended, and `take` returns the next item or
	throws. A stream supports one reader at a time.
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

	/** Returns the item `hasNext` found, or throws the failure that ended the stream. **/
	public function next():T
		return take();

	// Waits in short slices with a sleep between them, as `Future.await` does.
	function settle():Int {
		var flow = wait(20);
		while (flow == 0) {
			Sys.sleep(0.001);
			flow = wait(20);
		}
		return flow;
	}

	/**
		Calls `onItem` with each item and `onEnd` when the stream ends. The
		calls are made when the library's results are delivered: see `Loop`.
		A failure that ends the stream is passed to `onError` instead, or is
		thrown from that delivery if there is no `onError`.
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
