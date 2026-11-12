package hxn;

import hxn.reactive.Node;

/**
	Fine-grained reactivity in the style of SolidJS: `Signal` holds a value,
	`Computed` derives one, and an effect is code that reruns when a value it
	read changes. Dependencies are tracked dynamically: they are the values
	the code read on its last run. A signal that is read only under a
	condition is a dependency only while that condition holds.

	A graph is used only from the thread that builds it.
**/
class Reactive {
	/**
		Runs `body` now, and again whenever a signal or computed value it read
		has changed. Returns a function that disposes the effect.

		Effects and computed values created during a run are owned by the
		effect and disposed before its next run. Functions registered with
		`onCleanup` run at the same point.
	**/
	public static function effect(body:() -> Void):() -> Void {
		var node = new Node(null, () -> {
			body();
			return null;
		}, true);
		node.refresh();
		// Runs the effects queued by signals the first run set.
		Node.settle();
		return node.dispose;
	}

	/** Runs `body` and defers effects until it returns, so several sets cause one rerun. **/
	public static function batch<T>(body:() -> T):T {
		Node.batching++;
		var result:T;
		try {
			result = body();
		} catch (error:haxe.Exception) {
			Node.batching--;
			throw error;
		}
		Node.batching--;
		Node.settle();
		return result;
	}

	/** Runs `body` without subscribing the running effect to what it reads. **/
	public static function untrack<T>(body:() -> T):T {
		var lastCurrent = Node.current, lastGets = Node.gets, lastIndex = Node.getsIndex;
		Node.current = null;
		var result:T;
		try {
			result = body();
		} catch (error:haxe.Exception) {
			Node.current = lastCurrent;
			Node.gets = lastGets;
			Node.getsIndex = lastIndex;
			throw error;
		}
		Node.current = lastCurrent;
		Node.gets = lastGets;
		Node.getsIndex = lastIndex;
		return result;
	}

	/**
		Registers `cleanup` to run before the running effect or computed value
		next reruns, and when it is disposed. Outside an effect or computed
		value, `cleanup` is discarded.
	**/
	public static function onCleanup(cleanup:() -> Void):Void {
		var owner = Node.owner;
		if (owner == null)
			return;
		if (owner.cleanups == null)
			owner.cleanups = [cleanup];
		else
			owner.cleanups.push(cleanup);
	}

	/**
		Runs `body` in a new scope that owns the effects and computed values
		it creates, independently of any effect that was running when `root`
		was called. `body` receives a function that disposes them all. Nothing
		else disposes them.
	**/
	public static function root<T>(body:(dispose:() -> Void) -> T):T {
		var scope = new Node(null, null, false);
		var lastOwner = Node.owner, lastCurrent = Node.current, lastGets = Node.gets, lastIndex = Node.getsIndex;
		Node.owner = scope;
		Node.current = null;
		var result:T;
		try {
			result = body(scope.dispose);
		} catch (error:haxe.Exception) {
			Node.owner = lastOwner;
			Node.current = lastCurrent;
			Node.gets = lastGets;
			Node.getsIndex = lastIndex;
			throw error;
		}
		Node.owner = lastOwner;
		Node.current = lastCurrent;
		Node.gets = lastGets;
		Node.getsIndex = lastIndex;
		return result;
	}

	/**
		Returns a signal that mirrors a value a native library keeps up to
		date. Effects and computed values depend on it like any other signal.
		Each time the library's results are delivered (see `Loop`), it is set
		to the library's latest value, however many changes happened since
		the previous delivery.

		A binding passes its library's `Inbox` and three functions. `watch`
		registers the value with the library's queue and returns its id.
		`wait` takes the version last seen (negative for none) and a timeout
		in milliseconds. It returns the value's current version, the version
		it was passed if the timeout expired, or -1 once the value will not
		change again. `read` returns the value.

		The signal is updated until the library is done with the value or the
		owner `follow` was called under is disposed. Call `follow` inside a
		`root` to be able to stop it. An app with its own reactive graph can
		use `Inbox.follow`, which this is built on, and set its own signal.
	**/
	public static function follow<T>(inbox:Inbox, watch:() -> Int, wait:(seen:Int, limit:Int) -> Int, read:() -> T):Signal<T> {
		// The value is registered first, and its version is read before the
		// value itself, so that no change from this point on is missed.
		var id = watch();
		var seen = wait(-2, 0);
		var signal = new Signal(read());
		onCleanup(inbox.follow(id, seen, wait, read, signal.set));
		return signal;
	}
}
