package hxn;

/**
	An integer that a native library and Haxe both read and write, from any
	thread, without a call between them.

	The cell is four bytes that this runtime's own atomic operations and the
	library's both work on. One made here with `new` can be lent to a native
	call; one a library returns lives in the library and is read and written
	from here directly. The operations are those of `haxe.atomic.AtomicInt`.
**/
class AtomicInt {
	#if hl
	var cell:hl.Ref<Int>;
	#elseif cpp
	var cell:haxe.atomic.AtomicInt;
	#else
	#error "hxn.AtomicInt needs HashLink or hxcpp"
	#end

	// Whatever owns the memory, so it lives as long as this does.
	var keep:Any;

	/** What a binding passes to a native call that takes the cell. **/
	@:noCompletion public var pointer(get, never):#if hl hl.Ref<Int> #else cpp.Pointer<Int> #end;

	public function new(value:Int = 0) {
		#if hl
		var memory = new hl.NativeArray<Int>(1);
		memory[0] = value;
		cell = memory.getRef();
		keep = memory;
		#else
		var memory = [value];
		cell = cast cpp.Pointer.ofArray(memory);
		keep = memory;
		#end
	}

	/** The cell at `pointer`, which lives as long as `keep` is referenced. **/
	@:noCompletion public static function over(pointer:#if hl hl.Ref<Int> #else cpp.Pointer<Int> #end, keep:Any):AtomicInt {
		var made:AtomicInt = Type.createEmptyInstance(AtomicInt);
		@:privateAccess {
			made.cell = cast pointer;
			made.keep = keep;
		}
		return made;
	}

	inline function get_pointer()
		return cast cell;

	public inline function load():Int
		return #if hl hl.Atomics.load32(cell) #else cell.load() #end;

	/** Sets the value and returns it. **/
	public inline function store(value:Int):Int
		return #if hl hl.Atomics.store32(cell, value) #else cell.store(value) #end;

	/** Adds, and returns the value before. **/
	public inline function add(by:Int):Int
		return #if hl hl.Atomics.add32(cell, by) #else cell.add(by) #end;

	/** Subtracts, and returns the value before. **/
	public inline function sub(by:Int):Int
		return #if hl hl.Atomics.sub32(cell, by) #else cell.sub(by) #end;

	public inline function and(bits:Int):Int
		return #if hl hl.Atomics.and32(cell, bits) #else cell.and(bits) #end;

	public inline function or(bits:Int):Int
		return #if hl hl.Atomics.or32(cell, bits) #else cell.or(bits) #end;

	public inline function xor(bits:Int):Int
		return #if hl hl.Atomics.xor32(cell, bits) #else cell.xor(bits) #end;

	/** Sets the value and returns the one before. **/
	public inline function exchange(value:Int):Int
		return #if hl hl.Atomics.exchange32(cell, value) #else cell.exchange(value) #end;

	/** Sets the value if it is `expected`, and returns what it was either way. **/
	public inline function compareExchange(expected:Int, replacement:Int):Int
		return #if hl hl.Atomics.compareExchange32(cell, expected, replacement) #else cell.compareExchange(expected, replacement) #end;
}
