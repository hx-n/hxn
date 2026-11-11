package hxn;

/**
	A flag that a native library and Haxe both set and read, from any thread,
	without a call between them: an `AtomicInt` that is 0 or 1. Cancelling a
	call in progress is what it is usually for.
**/
class AtomicBool {
	var cell:AtomicInt;

	/** What a binding passes to a native call that takes the flag. **/
	@:noCompletion public var pointer(get, never):#if hl hl.Ref<Int> #else cpp.Pointer<Int> #end;

	public function new(value:Bool = false)
		cell = new AtomicInt(value ? 1 : 0);

	/** The flag in `cell`. **/
	@:noCompletion public static function over(cell:AtomicInt):AtomicBool {
		var made:AtomicBool = Type.createEmptyInstance(AtomicBool);
		@:privateAccess made.cell = cell;
		return made;
	}

	inline function get_pointer()
		return cell.pointer;

	public inline function load():Bool
		return cell.load() != 0;

	/** Sets the flag and returns it. **/
	public inline function store(value:Bool):Bool {
		cell.store(value ? 1 : 0);
		return value;
	}

	/** Sets the flag and returns what it was. **/
	public inline function exchange(value:Bool):Bool
		return cell.exchange(value ? 1 : 0) != 0;

	/** Sets the flag if it is `expected`, and returns what it was either way. **/
	public inline function compareExchange(expected:Bool, replacement:Bool):Bool
		return cell.compareExchange(expected ? 1 : 0, replacement ? 1 : 0) != 0;
}
