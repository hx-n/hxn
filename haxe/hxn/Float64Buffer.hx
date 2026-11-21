package hxn;

/**
	A fixed number of 64-bit floats in one block of memory.

	A library function that takes `&[f64]` reads the buffer for the duration
	of the call, and a function that takes `&mut [f64]` writes it. When the
	memory is aligned for `f64`, nothing is copied. The library cannot keep
	the buffer after the call returns.

	The length does not change. `resized` returns a new buffer and does not
	change this one.

	Indexing checks bounds only when you compile with `-debug`.
**/
abstract Float64Buffer(haxe.io.Bytes) {
	/** The size of one element in bytes. **/
	public static inline final BYTES = 8;

	/** The number of elements. **/
	public var length(get, never):Int;

	/** The bytes that hold the elements, in little-endian order. **/
	public var bytes(get, never):haxe.io.Bytes;

	/** A buffer of `length` elements, each zero. **/
	public inline function new(length:Int) {
		var bytes = haxe.io.Bytes.alloc(length << 3);
		bytes.fill(0, bytes.length, 0);
		this = bytes;
	}

	/**
		The elements `bytes` holds, without copying them: changes to either
		are seen by the other. Throws when the length of `bytes` is not a
		whole number of elements.
	**/
	public static function ofBytes(bytes:haxe.io.Bytes):Float64Buffer {
		if (bytes.length % BYTES != 0)
			throw new haxe.exceptions.ArgumentException("bytes", 'a length of ${bytes.length} is not a whole number of 8-byte elements');
		return cast bytes;
	}

	/** A buffer holding a copy of `values`. **/
	@:from public static function fromArray(values:Array<Float>):Float64Buffer {
		var out = new Float64Buffer(values.length);
		for (i in 0...values.length)
			out.set(i, values[i]);
		return out;
	}

	/** A copy of the elements. **/
	public function toArray():Array<Float> {
		return [for (i in 0...length) get(i)];
	}

	@:arrayAccess public inline function get(index:Int):Float {
		var p = at(index);
		#if hl
		var b:hl.Bytes = @:privateAccess this.b;
		return b.getF64(p);
		#elseif cpp
		return untyped __global__.__hxcpp_memory_get_double(this.getData(), p);
		#else
		return this.getDouble(p);
		#end
	}

	@:arrayAccess public inline function set(index:Int, value:Float):Float {
		var p = at(index);
		#if hl
		var b:hl.Bytes = @:privateAccess this.b;
		b.setF64(p, value);
		#elseif cpp
		untyped __global__.__hxcpp_memory_set_double(this.getData(), p, value);
		#else
		this.setDouble(p, value);
		#end
		return value;
	}

	/** Sets `count` elements from `start` to `value`, or every element from `start`. **/
	public function fill(value:Float, start:Int = 0, ?count:Int):Void {
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
	public inline function blit(start:Int, source:Float64Buffer, sourceStart:Int, count:Int):Void {
		this.blit(start << 3, source.bytes, sourceStart << 3, count << 3);
	}

	/**
		Returns a new buffer of `length` elements. It starts with the elements
		of this buffer, and any elements after them are zero.
	**/
	public function resized(length:Int):Float64Buffer {
		var out = new Float64Buffer(length);
		var kept = length < this.length >> 3 ? length : this.length >> 3;
		out.blit(0, cast this, 0, kept);
		return out;
	}

	inline function get_length():Int {
		return this.length >> 3;
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
		return index << 3;
	}
}
