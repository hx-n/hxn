package hxn;

import haxe.io.Bytes;
import haxe.io.BytesBuffer;
import haxe.io.BytesInput;

/**
	What a library's generated data classes share when they write a value's
	parts into bytes and read them back: the parts that are not one call of
	`haxe.io`. The layout is the one `hxn_abi::wire` describes.
**/
@:noCompletion class Wire {
	/** A length, then the text as UTF-8. **/
	public static function text(out:BytesBuffer, value:String):Void {
		if (value == null)
			throw new haxe.Exception("null where text is needed");
		bytes(out, Bytes.ofString(value));
	}

	/** A length, then the bytes. **/
	public static function bytes(out:BytesBuffer, value:Bytes):Void {
		if (value == null)
			throw new haxe.Exception("null where bytes are needed");
		out.addInt32(value.length);
		out.add(value);
	}

	public static function readBytes(input:BytesInput):Bytes {
		var length = input.readInt32();
		var value = Bytes.alloc(length);
		if (length > 0)
			input.readFullBytes(value, 0, length);
		return value;
	}

	public static function readText(input:BytesInput):String {
		var value = readBytes(input);
		return value.getString(0, value.length);
	}

	/** Eight bytes, the low four first. **/
	public static function readInt64(input:BytesInput):haxe.Int64 {
		var low = input.readInt32();
		var high = input.readInt32();
		return haxe.Int64.make(high, low);
	}
}
