package hxn;

import hxn.reactive.Node;

/**
	Fine-grained reactivity in the style of SolidJS: `Signal` holds a value,
	`Computed` derives one, and an effect is code that reruns when what it
	read changes. Dependencies are found by running the code, so they follow
	its branches: a signal read only when a condition holds is a dependency
	only while it does.

	A graph belongs to the thread that builds it.
**/
class Reactive {
	/**
		Runs `body` now, and again whenever a signal or computed value it read
		has changed. Returns what stops it.

		Effects and computed values made during a run belong to it and are
		disposed before the next run, as is whatever `onCleanup` was given.
	**/
	public static function effect(body:() -> Void):() -> Void {
		var node = new Node(null, () -> {
			body();
			return null;
		}, true);
		node.refresh();
		// For what the first run itself set.
		Node.settle();
		return node.dispose;
	}

	/** Runs `body`, holding back effects until it returns, so several sets cause one rerun. **/
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
		Has `cleanup` run before the running effect or computed value next
		reruns, and when it is disposed. Outside one it is not kept.
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
		Runs `body` as the owner of the effects and computed values it makes,
		whatever was running when it was called. It is given what disposes them
		all, which nothing else will.
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
		A signal that follows a value a native library keeps current. Effects
		and computed values depend on it like any other. It is set with the
		library's latest value, however many changes came between, when the
		library's results are next delivered: see `Loop`.

		A binding passes its library's `Inbox` and three calls. `watch` starts
		the value being noted there and returns its id. `wait` takes the
		version last seen (negative for none) and a limit in milliseconds, and
		returns the version the value is at, the one it was given if the time
		ran out, or -1 once the value will not change again. `read` returns
		the value.

		Following lasts until the library is done with the value or the owner
		this was called in is disposed, so follow inside a `root` to be able
		to stop. An app with a graph of its own can use `Inbox.follow`, which
		this is built on, and set its own signal.
	**/
	public static function follow<T>(inbox:Inbox, watch:() -> Int, wait:(seen:Int, limit:Int) -> Int, read:() -> T):Signal<T> {
		// Noted first and the version read before the value, so a change at
		// any point from here on is seen, not missed.
		var id = watch();
		var seen = wait(-2, 0);
		var signal = new Signal(read());
		onCleanup(inbox.follow(id, seen, wait, read, signal.set));
		return signal;
	}
}
