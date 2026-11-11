package hxn;

/**
	Collection, for memory the collector cannot see.

	A native value is released by `close()`, or by the collector when its
	object is no longer reachable. The collector runs when the memory it
	manages has grown, and an object holding a native value is a few words of
	that whatever the value holds. Two things make up for it:

	- `collect` runs a collection now, the same call on every runtime;
	- a library declares what its values hold, and its `Meter` collects when that
	  has grown. A binding checks the meter whenever it makes an object of an
	  interface the library marked as weighed.

	`close()` is still the way to release something at a known moment.
**/
class Gc {
	/**
		How many bytes a library's values may hold before a collection is run
		for them. After one, the next is run at twice what was left, if that
		is more.
	**/
	public static var budget:Float = 64 * 1024 * 1024;

	@:allow(hxn) static final meters:Array<Meter> = [];

	/**
		Runs a full collection now. False where the runtime cannot be made
		to run one: hxcpp under Emscripten.
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
		Checks every library's meter. `Loop.sync` does this, and so does a
		binding when it makes a weighed object.
	**/
	public static function check():Void {
		for (meter in meters)
			meter.check();
	}
}

/**
	What one library's values hold outside the collector's sight, and the
	decision to collect for it. A binding makes one for its library.
**/
class Meter {
	final read:() -> Float;
	final wanted:() -> Bool;

	// Twice what was held after the last collection run from here.
	var floor = 0.;

	#if !(emscripten || hxn_no_threads)
	final turn = new sys.thread.Mutex();
	#end

	/**
		`held` gives the bytes the library's values declare, and `wanted`
		whether the library asked for a collection since it was last called.
	**/
	public function new(held:() -> Float, wanted:() -> Bool) {
		read = held;
		this.wanted = wanted;
		Gc.meters.push(this);
	}

	/** The bytes the library's live values declare. **/
	public var held(get, never):Float;

	inline function get_held():Float
		return read();

	/**
		Collects if the library asked, or holds more than it may. Called on a
		Haxe thread, outside a collection.
	**/
	public function check():Void {
		#if !(emscripten || hxn_no_threads)
		// A thread that is already checking speaks for this one.
		if (!turn.tryAcquire())
			return;
		#end
		// Asked first, so that a request is taken whatever is held.
		if (wanted() || read() > Math.max(Gc.budget, floor)) {
			Gc.collect();
			floor = read() * 2;
		}
		#if !(emscripten || hxn_no_threads)
		turn.release();
		#end
	}
}
