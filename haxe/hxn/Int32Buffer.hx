package hxn;

/**
	A fixed number of signed 32-bit integers, stored together in one block of memory.

	A library function that takes `&[i32]` reads the buffer in place, and one
	that takes `&mut [i32]` writes it in place, for the duration of the call.
	Nothing is copied, and the library cannot keep the buffer after the call
	returns.

	The length never changes. `resized` makes a new buffer, which replaces
	this one wherever it is used.

	Indexing is not bounds-checked unless compiled with `-debug`.
**/
abstract Int32Buffer(haxe.io.Bytes) {
	/** The size of one element in bytes. **/
	public static inline final BYTES = 4;

	/** The number of elements. **/
	public var length(get, never):Int;

	/** The bytes that hold the elements, in little-endian order. **/
	public var bytes(get, never):haxe.io.Bytes;

	/** A buffer of `length` elements, each zero. **/
	public inline function new(length:Int) {
		var bytes = haxe.io.Bytes.alloc(length << 2);
		bytes.fill(0, bytes.length, 0);
		this = bytes;
	}

	/**
		The elements `bytes` holds, without copying them: changes to either
		are seen by the other. Throws when the length of `bytes` is not a
		whole number of elements.
	**/
	public static function ofBytes(bytes:haxe.io.Bytes):Int32Buffer {
		if (bytes.length % BYTES != 0)
			throw new haxe.exceptions.ArgumentException("bytes", 'a length of ${bytes.length} is not a whole number of 4-byte elements');
		return cast bytes;
	}

	/** A buffer holding a copy of `values`. **/
	@:from public static function fromArray(values:Array<Int>):Int32Buffer {
		var out = new Int32Buffer(values.length);
		for (i in 0...values.length)
			out.set(i, values[i]);
		return out;
	}

	/** A copy of the elements. **/
	public function toArray():Array<Int> {
		return [for (i in 0...length) get(i)];
	}

	@:arrayAccess public inline function get(index:Int):Int {
		var p = at(index);
		#if hl
		var b:hl.Bytes = @:privateAccess this.b;
		return b.getI32(p);
		#elseif cpp
		return untyped __global__.__hxcpp_memory_get_i32(this.getData(), p);
		#else
		return this.getInt32(p);
		#end
	}

	@:arrayAccess public inline function set(index:Int, value:Int):Int {
		var p = at(index);
		#if hl
		var b:hl.Bytes = @:privateAccess this.b;
		b.setI32(p, value);
		#elseif cpp
		untyped __global__.__hxcpp_memory_set_i32(this.getData(), p, value);
		#else
		this.setInt32(p, value);
		#end
		return value;
	}

	/** Sets `count` elements from `start` to `value`, or every element from `start`. **/
	public function fill(value:Int, start:Int = 0, ?count:Int):Void {
		var end = count == null ? length : start + count;
		if (start < 0 || end > length || start > end)
			throw haxe.io.Error.OutsideBounds;
		for (i in start...end)
			set(i, value);
	}

	/**
		Copies `count` elements of `source`, from `sourceStart`, to this
		buffer at `start`. The two may be the same buffer.
	**/
	public inline function blit(start:Int, source:Int32Buffer, sourceStart:Int, count:Int):Void {
		this.blit(start << 2, source.bytes, sourceStart << 2, count << 2);
	}

	/**
		A new buffer of `length` elements that starts with this one's. Any
		elements past this buffer's length are zero.
	**/
	public function resized(length:Int):Int32Buffer {
		var out = new Int32Buffer(length);
		var kept = length < this.length >> 2 ? length : this.length >> 2;
		out.blit(0, cast this, 0, kept);
		return out;
	}

	inline function get_length():Int {
		return this.length >> 2;
	}

	inline function get_bytes():haxe.io.Bytes {
		return this;
	}

	// The byte offset of the element at `index`.
	inline function at(index:Int):Int {
		#if debug
		if (index < 0 || index >= length)
			throw haxe.io.Error.OutsideBounds;
		#end
		return index << 2;
	}
}
