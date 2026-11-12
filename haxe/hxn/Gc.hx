package hxn;

/**
	Garbage collection control for native memory, which the collector cannot
	measure.

	A native value is released by `close()`, or by the collector when its
	Haxe object is no longer reachable. The collector runs when the memory it
	manages has grown. A Haxe object that holds a native value counts as a
	few words of that memory, however much native memory the value holds. Two
	mechanisms compensate:

	- `collect` runs a collection immediately, with the same call on every
	  runtime;
	- a library reports how many bytes its values hold, and its `Meter` runs
	  a collection when that total has grown. A binding checks the meter
	  whenever it creates an object of an interface the library marked as
	  weighed.

	`close()` remains the way to release a value at a known time.
**/
class Gc {
	/**
		The number of bytes a library's values may hold before a collection is
		run for them. After a collection, the next one runs at twice the bytes
		still held, if that is more than this.
	**/
	public static var budget:Float = 64 * 1024 * 1024;

	@:allow(hxn) static final meters:Array<Meter> = [];

	/**
		Runs a full collection now. Returns false where the runtime cannot be
		forced to run one, which is hxcpp under Emscripten.
	**/
	public static function collect():Bool {
		#if hl
		hl.Gc.major();
		return true;
		#elseif (cpp && !emscripten)
		cpp.vm.Gc.run(true);
		return true;
		#else
		return false;
		#end
	}

	/**
		Checks every library's meter. `Loop.sync` calls this, and so does a
		binding when it creates a weighed object.
	**/
	public static function check():Void {
		for (meter in meters)
			meter.check();
	}
}

/**
	Tracks the memory one library's values hold, which the collector cannot
	measure, and decides when to run a collection for it. A binding creates
	one for its library.
**/
class Meter {
	final read:() -> Float;
	final wanted:() -> Bool;

	// Twice the bytes held after the last collection this meter ran.
	var floor = 0.;

	#if !(emscripten || hxn_no_threads)
	final turn = new sys.thread.Mutex();
	#end

	/**
		`held` returns the number of bytes the library's values report.
		`wanted` returns whether the library has requested a collection since
		`wanted` was last called.
	**/
	public function new(held:() -> Float, wanted:() -> Bool) {
		read = held;
		this.wanted = wanted;
		Gc.meters.push(this);
	}

	/** The number of bytes the library's live values report. **/
	public var held(get, never):Float;

	inline function get_held():Float
		return read();

	/**
		Runs a collection if the library requested one or its values hold more
		than the limit. Called on a Haxe thread, outside a collection.
	**/
	public function check():Void {
		#if !(emscripten || hxn_no_threads)
		// If another thread is already checking, its check covers this one.
		if (!turn.tryAcquire())
			return;
		#end
		// `wanted` is evaluated first so that a request is consumed regardless of the bytes held.
		if (wanted() || read() > Math.max(Gc.budget, floor)) {
			Gc.collect();
			floor = read() * 2;
		}
		#if !(emscripten || hxn_no_threads)
		turn.release();
		#end
	}
}
