package hxn;

/**
	A fixed number of unsigned 16-bit integers in one block of memory.

	A library function that takes `&[u16]` reads the buffer for the duration
	of the call, and a function that takes `&mut [u16]` writes it. When the
	memory is aligned for `u16`, nothing is copied. The library cannot keep
	the buffer after the call returns.

	The length does not change. `resized` returns a new buffer and does not
	change this one.

	When you set a value, the buffer keeps only its low 16 bits.

	Indexing checks bounds only when you compile with `-debug`.
**/
abstract UInt16Buffer(haxe.io.Bytes) {
	/** The size of one element in bytes. **/
	public static inline final BYTES = 2;

	/** The number of elements. **/
	public var length(get, never):Int;

	/** The bytes that hold the elements, in little-endian order. **/
	public var bytes(get, never):haxe.io.Bytes;

	/** A buffer of `length` elements, each zero. **/
	public inline function new(length:Int) {
		var bytes = haxe.io.Bytes.alloc(length << 1);
		bytes.fill(0, bytes.length, 0);
		this = bytes;
	}

	/**
		The elements `bytes` holds, without copying them: changes to either
		are seen by the other. Throws when the length of `bytes` is not a
		whole number of elements.
	**/
	public static function ofBytes(bytes:haxe.io.Bytes):UInt16Buffer {
		if (bytes.length % BYTES != 0)
			throw new haxe.exceptions.ArgumentException("bytes", 'a length of ${bytes.length} is not a whole number of 2-byte elements');
		return cast bytes;
	}

	/** A buffer holding a copy of `values`. **/
	@:from public static function fromArray(values:Array<Int>):UInt16Buffer {
		var out = new UInt16Buffer(values.length);
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
		return b.getUI16(p);
		#elseif cpp
		return untyped __global__.__hxcpp_memory_get_ui16(this.getData(), p);
		#else
		return this.getUInt16(p);
		#end
	}

	@:arrayAccess public inline function set(index:Int, value:Int):Int {
		var p = at(index);
		#if hl
		var b:hl.Bytes = @:privateAccess this.b;
		b.setUI16(p, value & 0xffff);
		#elseif cpp
		untyped __global__.__hxcpp_memory_set_ui16(this.getData(), p, value);
		#else
		this.setUInt16(p, value & 0xffff);
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
	public inline function blit(start:Int, source:UInt16Buffer, sourceStart:Int, count:Int):Void {
		this.blit(start << 1, source.bytes, sourceStart << 1, count << 1);
	}

	/**
		Returns a new buffer of `length` elements. It starts with the elements
		of this buffer, and any elements after them are zero.
	**/
	public function resized(length:Int):UInt16Buffer {
		var out = new UInt16Buffer(length);
		var kept = length < this.length >> 1 ? length : this.length >> 1;
		out.blit(0, cast this, 0, kept);
		return out;
	}

	inline function get_length():Int {
		return this.length >> 1;
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
		return index << 1;
	}
}
