package hxn;

/**
	A fixed number of signed 16-bit integers in one block of memory.

	A library function that takes `&[i16]` reads the buffer for the duration
	of the call, and a function that takes `&mut [i16]` writes it. When the
	memory is aligned for `i16`, nothing is copied. The library cannot keep
	the buffer after the call returns.

	The length does not change. `resized` returns a new buffer and does not
	change this one.

	When you set a value, the buffer keeps only its low 16 bits.

	Indexing checks bounds only when you compile with `-debug`.
**/
abstract Int16Buffer(#if hl hl.Bytes #elseif cpp Array<cpp.UInt8> #else Array<Int> #end) {
	/** The size of one element in bytes. **/
	public static inline final BYTES = 2;

	#if hl
	// The element count, then padding that keeps the elements 16-byte aligned.
	// The Rust adapter reads the same layout.
	static inline final HEADER = 16;
	#end

	/** The number of elements. **/
	public var length(get, never):Int;

	/** A buffer of `length` elements, each zero. **/
	public inline function new(length:Int) {
		if (length < 0)
			throw new haxe.exceptions.ArgumentException("length", 'a buffer cannot have $length elements');
		#if hl
		var size = HEADER + (length << 1);
		var b = new hl.Bytes(size);
		b.fill(0, size, 0);
		b.setI32(0, length);
		this = b;
		#elseif cpp
		// Two bytes for each element, so that the binding lends them as they are.
		var a:Array<cpp.UInt8> = cpp.NativeArray.create(length << 1);
		#if !cppia
		cpp.NativeArray.zero(a);
		#end
		this = a;
		#else
		this = [for (_ in 0...length) 0];
		#end
	}

	/** A buffer holding a copy of `values`. **/
	@:from public static function fromArray(values:Array<Int>):Int16Buffer {
		var out = new Int16Buffer(values.length);
		for (i in 0...values.length)
			out.set(i, values[i]);
		return out;
	}

	/**
		A buffer holding the elements that `bytes` stores in little-endian
		order. Throws when the length of `bytes` is not a whole number of
		elements.
	**/
	public static function fromBytes(bytes:haxe.io.Bytes):Int16Buffer {
		if (bytes.length % BYTES != 0)
			throw new haxe.exceptions.ArgumentException("bytes", 'a length of ${bytes.length} is not a whole number of 2-byte elements');
		var out = new Int16Buffer(bytes.length >> 1);
		for (i in 0...out.length)
			out.set(i, (bytes.getUInt16(i << 1) << 16) >> 16);
		return out;
	}

	/** A copy of the elements. **/
	public function toArray():Array<Int> {
		return [for (i in 0...length) get(i)];
	}

	/** A copy of the elements in little-endian order. **/
	public function toBytes():haxe.io.Bytes {
		var out = haxe.io.Bytes.alloc(length << 1);
		for (i in 0...length)
			out.setUInt16(i << 1, get(i) & 0xffff);
		return out;
	}

	@:arrayAccess public inline function get(index:Int):Int {
		check(index);
		#if hl
		var p = HEADER + (index << 1);
		return (this.getUI16(p) << 16) >> 16;
		#elseif cppia
		return (((this[index << 1] | (this[(index << 1) + 1] << 8)) << 16) >> 16);
		#elseif cpp
		return untyped __global__.__hxcpp_memory_get_i16(this, index << 1);
		#else
		return this[index];
		#end
	}

	@:arrayAccess public inline function set(index:Int, value:Int):Int {
		check(index);
		#if hl
		var p = HEADER + (index << 1);
		this.setUI16(p, value & 0xffff);
		#elseif cppia
		this[index << 1] = value & 0xff;
		this[(index << 1) + 1] = (value >> 8) & 0xff;
		#elseif cpp
		untyped __global__.__hxcpp_memory_set_i16(this, index << 1, value);
		#else
		this[index] = (value << 16) >> 16;
		#end
		return value;
	}

	/** Sets `count` elements from `start` to `value`. **/
	// No optional arguments: HashLink boxes an optional Int on every call.
	public function fill(value:Int, start:Int, count:Int):Void {
		var end = start + count;
		if (start < 0 || count < 0 || end > length)
			throw haxe.io.Error.OutsideBounds;
		for (i in start...end)
			set(i, value);
	}

	/**
		Copies `count` elements of `source`, from `sourceStart`, to this
		buffer at `start`. The two may be the same buffer.
	**/
	public function blit(start:Int, source:Int16Buffer, sourceStart:Int, count:Int):Void {
		if (start < 0 || sourceStart < 0 || count < 0 || start + count > length || sourceStart + count > source.length)
			throw haxe.io.Error.OutsideBounds;
		#if hl
		this.blit(HEADER + (start << 1), source.hxnData(), HEADER + (sourceStart << 1), count << 1);
		#elseif (cpp && !cppia)
		cpp.NativeArray.blit(this, start << 1, source.hxnData(), sourceStart << 1, count << 1);
		#else
		var from = source.hxnData();
		#if cppia
		start <<= 1;
		sourceStart <<= 1;
		count <<= 1;
		#end
		if (start <= sourceStart)
			for (i in 0...count)
				this[start + i] = from[sourceStart + i];
		else {
			var i = count;
			while (i-- > 0)
				this[start + i] = from[sourceStart + i];
		}
		#end
	}

	/**
		Returns a new buffer of `length` elements. It starts with the elements
		of this buffer, and any elements after them are zero.
	**/
	public function resized(length:Int):Int16Buffer {
		var out = new Int16Buffer(length);
		var current = get_length();
		out.blit(0, cast this, 0, length < current ? length : current);
		return out;
	}

	#if hl
	/**
		The `hl.Bytes` that hold this buffer on HashLink and Ash, for `ash.simd`
		loads and stores. `hlOffset` gives the byte offset of an element in it.
	**/
	public inline function hlBytes():hl.Bytes {
		return this;
	}

	/** The byte offset in `hlBytes()` of the element at `index`. **/
	public static inline function hlOffset(index:Int):Int {
		return HEADER + (index << 1);
	}
	#end

	/** The storage that the generated bindings pass to native code. **/
	@:noCompletion public inline function hxnData():#if hl hl.Bytes #elseif cpp Array<cpp.UInt8> #else Array<Int> #end {
		return this;
	}

	inline function get_length():Int {
		#if hl
		return this.getI32(0);
		#elseif cpp
		return this.length >> 1;
		#else
		return this.length;
		#end
	}

	inline function check(index:Int):Void {
		#if debug
		if (index < 0 || index >= get_length())
			throw haxe.io.Error.OutsideBounds;
		#end
	}
}
