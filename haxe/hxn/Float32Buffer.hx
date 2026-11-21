package hxn;

// A 32-bit float where the target has one.
private typedef Element = #if (hl || cpp) Single #else Float #end;

/**
	A fixed number of 32-bit floats in one block of memory.

	A library function that takes `&[f32]` reads the buffer for the duration
	of the call, and a function that takes `&mut [f32]` writes it. When the
	memory is aligned for `f32`, nothing is copied. The library cannot keep
	the buffer after the call returns.

	The length does not change. `resized` returns a new buffer and does not
	change this one.

	Indexing checks bounds only when you compile with `-debug`.
**/
abstract Float32Buffer(#if hl hl.Bytes #elseif cpp Array<Single> #else Array<Float> #end) {
	/** The size of one element in bytes. **/
	public static inline final BYTES = 4;

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
		var size = HEADER + (length << 2);
		var b = new hl.Bytes(size);
		b.fill(0, size, 0);
		b.setI32(0, length);
		this = b;
		#elseif cpp
		var a:Array<Single> = cpp.NativeArray.create(length);
		#if !cppia
		cpp.NativeArray.zero(a);
		#end
		this = a;
		#else
		this = [for (_ in 0...length) 0];
		#end
	}

	/** A buffer holding a copy of `values`. **/
	@:from public static function fromArray(values:Array<Float>):Float32Buffer {
		var out = new Float32Buffer(values.length);
		for (i in 0...values.length)
			out.set(i, values[i]);
		return out;
	}

	/**
		A buffer holding the elements that `bytes` stores in little-endian
		order. Throws when the length of `bytes` is not a whole number of
		elements.
	**/
	public static function fromBytes(bytes:haxe.io.Bytes):Float32Buffer {
		if (bytes.length % BYTES != 0)
			throw new haxe.exceptions.ArgumentException("bytes", 'a length of ${bytes.length} is not a whole number of 4-byte elements');
		var out = new Float32Buffer(bytes.length >> 2);
		for (i in 0...out.length)
			out.set(i, bytes.getFloat(i << 2));
		return out;
	}

	/** A copy of the elements. **/
	public function toArray():Array<Float> {
		return [for (i in 0...length) get(i)];
	}

	/** A copy of the elements in little-endian order. **/
	public function toBytes():haxe.io.Bytes {
		var out = haxe.io.Bytes.alloc(length << 2);
		for (i in 0...length)
			out.setFloat(i << 2, get(i));
		return out;
	}

	@:arrayAccess public inline function get(index:Int):Element {
		check(index);
		#if hl
		var p = HEADER + (index << 2);
		return this.getF32(p);
		#elseif cppia
		return this[index];
		#elseif cpp
		return cpp.NativeArray.unsafeGet(this, index);
		#else
		return this[index];
		#end
	}

	@:arrayAccess public inline function set(index:Int, value:Element):Element {
		check(index);
		#if hl
		var p = HEADER + (index << 2);
		this.setF32(p, value);
		#elseif cppia
		this[index] = value;
		#elseif cpp
		cpp.NativeArray.unsafeSet(this, index, value);
		#else
		this[index] = value;
		#end
		return value;
	}

	/** Sets `count` elements from `start` to `value`. **/
	// No optional arguments: HashLink boxes an optional Int on every call.
	public function fill(value:Element, start:Int, count:Int):Void {
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
	public function blit(start:Int, source:Float32Buffer, sourceStart:Int, count:Int):Void {
		if (start < 0 || sourceStart < 0 || count < 0 || start + count > length || sourceStart + count > source.length)
			throw haxe.io.Error.OutsideBounds;
		#if hl
		this.blit(HEADER + (start << 2), source.hxnData(), HEADER + (sourceStart << 2), count << 2);
		#elseif (cpp && !cppia)
		cpp.NativeArray.blit(this, start, source.hxnData(), sourceStart, count);
		#else
		var from = source.hxnData();
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
	public function resized(length:Int):Float32Buffer {
		var out = new Float32Buffer(length);
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
		return HEADER + (index << 2);
	}
	#end

	/** The storage that the generated bindings pass to native code. **/
	@:noCompletion public inline function hxnData():#if hl hl.Bytes #elseif cpp Array<Single> #else Array<Float> #end {
		return this;
	}

	inline function get_length():Int {
		#if hl
		return this.getI32(0);
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
