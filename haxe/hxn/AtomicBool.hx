package hxn;

/**
	A boolean flag shared by a native library and Haxe. Either side reads and
	writes it from any thread without a native call. It is an `AtomicInt`
	that holds 0 or 1, and is typically used to cancel a call in progress.
**/
class AtomicBool {
	var cell:AtomicInt;

	/** The pointer a binding passes to a native call that has the flag as a parameter. **/
	@:noCompletion public var pointer(get, never):#if hl hl.Ref<Int> #else cpp.Pointer<Int> #end;

	public function new(value:Bool = false)
		cell = new AtomicInt(value ? 1 : 0);

	/** Wraps `cell` as a flag. **/
	@:noCompletion public static function over(cell:AtomicInt):AtomicBool {
		var made:AtomicBool = Type.createEmptyInstance(AtomicBool);
		@:privateAccess made.cell = cell;
		return made;
	}

	inline function get_pointer()
		return cell.pointer;

	public inline function load():Bool
		return cell.load() != 0;

	/** Sets the flag and returns the new value. **/
	public inline function store(value:Bool):Bool {
		cell.store(value ? 1 : 0);
		return value;
	}

	/** Sets the flag and returns the previous value. **/
	public inline function exchange(value:Bool):Bool
		return cell.exchange(value ? 1 : 0) != 0;

	/** Sets the flag if it equals `expected`. Returns the previous value in both cases. **/
	public inline function compareExchange(expected:Bool, replacement:Bool):Bool
		return cell.compareExchange(expected ? 1 : 0, replacement ? 1 : 0) != 0;
}
