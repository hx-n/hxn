package hxn.reactive;

/**
	One vertex of the reactive graph: a signal, a computed value or an effect.
	`Signal`, `Computed` and `Reactive` are the API; this is the algorithm.

	A write marks what reads the signal as dirty and everything further down
	as needing a check, and queues the effects among them. Nothing is computed
	then. An effect that runs, or a computed value that is read, first brings
	its own sources up to date, and recomputes only if one of them really
	changed. So a value is computed once however many paths lead to it, and
	never from a mix of old and new.

	The graph belongs to one thread.
**/
@:allow(hxn)
class Node {
	static inline final CLEAN = 0;
	static inline final CHECK = 1;
	static inline final DIRTY = 2;
	static inline final DISPOSED = 3;

	// The computation that is running, which records what it reads.
	static var current:Null<Node> = null;
	// What it has read that its last run did not read at the same place, and
	// how far the two runs agreed before that.
	static var gets:Null<Array<Node>> = null;
	static var getsIndex = 0;
	// What new computations and cleanups belong to.
	static var owner:Null<Node> = null;

	static var queue:Array<Node> = [];
	static var batching = 0;
	static var flushing = false;

	var value:Dynamic;
	var compute:Null<() -> Dynamic>;
	var equals:Null<(Dynamic, Dynamic) -> Bool>;
	var state:Int;
	var isEffect:Bool;
	var sources:Null<Array<Node>>;
	var observers:Null<Array<Node>>;
	var owned:Null<Array<Node>>;
	var cleanups:Null<Array<() -> Void>>;

	function new(value:Dynamic, compute:Null<() -> Dynamic>, isEffect:Bool, ?equals:(Dynamic, Dynamic) -> Bool) {
		this.value = value;
		this.compute = compute;
		this.isEffect = isEffect;
		this.equals = equals;
		state = compute == null ? CLEAN : DIRTY;
		// Disposed with whatever was running when it was made.
		if (compute != null && owner != null) {
			if (owner.owned == null)
				owner.owned = [this];
			else
				owner.owned.push(this);
		}
	}

	inline function same(a:Dynamic, b:Dynamic):Bool
		return equals != null ? equals(a, b) : a == b;

	function read():Dynamic {
		if (current != null && state != DISPOSED) {
			var known = current.sources;
			if (gets == null && known != null && getsIndex < known.length && known[getsIndex] == this)
				getsIndex++;
			else if (gets == null)
				gets = [this];
			else
				gets.push(this);
		}
		if (compute != null)
			refresh();
		return value;
	}

	function write(next:Dynamic):Void {
		if (same(value, next))
			return;
		value = next;
		if (observers != null)
			for (observer in observers)
				observer.stale(DIRTY);
		settle();
	}

	function stale(to:Int):Void {
		if (state >= to)
			return;
		if (state == CLEAN && isEffect)
			queue.push(this);
		state = to;
		if (observers != null)
			for (observer in observers)
				observer.stale(CHECK);
	}

	// Brings this up to date: its sources first, then itself if one changed.
	function refresh():Void {
		if (state == DISPOSED)
			return;
		if (state == CHECK && sources != null) {
			for (source in sources) {
				source.refresh();
				if (state == DIRTY)
					break;
			}
		}
		if (state == DIRTY)
			run();
		if (state != DISPOSED)
			state = CLEAN;
	}

	function run():Void {
		var old = value;
		clean();
		var lastCurrent = current, lastGets = gets, lastIndex = getsIndex, lastOwner = owner;
		current = this;
		gets = null;
		getsIndex = 0;
		owner = this;
		try {
			value = compute();
		} catch (error:haxe.Exception) {
			// What it reads is as it was. A computed value stays dirty, to be
			// tried at its next read; an effect waits for the next change.
			current = lastCurrent;
			gets = lastGets;
			getsIndex = lastIndex;
			owner = lastOwner;
			if (isEffect && state != DISPOSED)
				state = CLEAN;
			throw error;
		}
		var read = gets, agreed = getsIndex;
		current = lastCurrent;
		gets = lastGets;
		getsIndex = lastIndex;
		owner = lastOwner;
		if (state == DISPOSED)
			return;

		// Keep the sources both runs read in the same order, and swap the rest.
		if (read != null) {
			unobserve(agreed);
			if (sources != null && agreed > 0) {
				sources.resize(agreed);
				for (source in read)
					sources.push(source);
			} else {
				sources = read;
			}
			for (i in agreed...sources.length) {
				var source = sources[i];
				if (source.observers == null)
					source.observers = [this];
				else
					source.observers.push(this);
			}
		} else if (sources != null && agreed < sources.length) {
			unobserve(agreed);
			sources.resize(agreed);
		}

		// What reads this was only told to check. Now it is known to have changed.
		if (observers != null && !same(old, value))
			for (observer in observers)
				if (observer.state != DISPOSED)
					observer.state = DIRTY;
		state = CLEAN;
	}

	function unobserve(from:Int):Void {
		if (sources == null)
			return;
		for (i in from...sources.length) {
			var watched = sources[i].observers;
			if (watched != null)
				watched.remove(this);
		}
	}

	// What a run set up goes before the next run, and when this is disposed.
	function clean():Void {
		if (owned != null) {
			var children = owned;
			owned = null;
			for (child in children)
				child.dispose();
		}
		if (cleanups != null) {
			var pending = cleanups;
			cleanups = null;
			var lastCurrent = current;
			current = null;
			for (cleanup in pending)
				cleanup();
			current = lastCurrent;
		}
	}

	function dispose():Void {
		if (state == DISPOSED)
			return;
		state = DISPOSED;
		clean();
		unobserve(0);
		sources = null;
		observers = null;
	}

	// Runs the queued effects, unless something further out will.
	static function settle():Void {
		if (batching > 0 || flushing || current != null)
			return;
		flushing = true;
		var ran = 0;
		try {
			while (ran < queue.length) {
				if (ran > 100000)
					throw new haxe.Exception("effects keep setting each other off");
				queue[ran++].refresh();
			}
		} catch (error:haxe.Exception) {
			queue.splice(0, ran);
			flushing = false;
			throw error;
		}
		queue.resize(0);
		flushing = false;
	}
}
