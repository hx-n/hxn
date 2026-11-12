package hxn;

/**
	An integer shared by a native library and Haxe. Either side reads and
	writes it from any thread without a native call.

	The cell is four bytes of memory that both this runtime's atomic
	operations and the library's operate on. A cell created here with `new`
	can be passed to a native call, which borrows it for the duration of the
	call. A cell returned by a library is in the library's memory and is read
	and written from Haxe directly. The operations are those of
	`haxe.atomic.AtomicInt`.
**/
class AtomicInt {
	#if hl
	var cell:hl.Ref<Int>;
	#elseif cpp
	var cell:haxe.atomic.AtomicInt;
	#else
	#error "hxn.AtomicInt needs HashLink or hxcpp"
	#end

	// The owner of the cell's memory, referenced so the memory lives as long as this object.
	var keep:Any;

	/** The pointer a binding passes to a native call that has the cell as a parameter. **/
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

	/** Wraps the cell at `pointer`. Its memory stays valid as long as `keep` is referenced. **/
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

	/** Sets the value and returns the new value. **/
	public inline function store(value:Int):Int
		return #if hl hl.Atomics.store32(cell, value) #else cell.store(value) #end;

	/** Adds `by` and returns the previous value. **/
	public inline function add(by:Int):Int
		return #if hl hl.Atomics.add32(cell, by) #else cell.add(by) #end;

	/** Subtracts `by` and returns the previous value. **/
	public inline function sub(by:Int):Int
		return #if hl hl.Atomics.sub32(cell, by) #else cell.sub(by) #end;

	public inline function and(bits:Int):Int
		return #if hl hl.Atomics.and32(cell, bits) #else cell.and(bits) #end;

	public inline function or(bits:Int):Int
		return #if hl hl.Atomics.or32(cell, bits) #else cell.or(bits) #end;

	public inline function xor(bits:Int):Int
		return #if hl hl.Atomics.xor32(cell, bits) #else cell.xor(bits) #end;

	/** Sets the value and returns the previous value. **/
	public inline function exchange(value:Int):Int
		return #if hl hl.Atomics.exchange32(cell, value) #else cell.exchange(value) #end;

	/** Sets the value if it equals `expected`. Returns the previous value in both cases. **/
	public inline function compareExchange(expected:Int, replacement:Int):Int
		return #if hl hl.Atomics.compareExchange32(cell, expected, replacement) #else cell.compareExchange(expected, replacement) #end;
}
