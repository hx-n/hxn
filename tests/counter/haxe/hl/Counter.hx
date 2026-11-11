private typedef Raw = hl.Abstract<"hxn_counter">;

@:hlNative("counter")
private class Native {
	public static function make(start:Int):Raw
		return null;

	public static function add(self:Raw, amount:Int):Int
		return 0;

	public static function label(self:Raw, name:String):hl.Bytes
		return null;

	public static function sum(self:Raw, data:hl.Bytes, length:Int):Int
		return 0;

	public static function close(self:Raw):Void {}

	public static function live():Int
		return 0;

	public static function fail():Void {}

	public static function nap(milliseconds:Int):Void {}
}

class Counter {
	final raw:Raw;

	public function new(start:Int)
		raw = Native.make(start);

	public function add(amount:Int):Int
		return Native.add(raw, amount);

	public function label(name:String):String
		return @:privateAccess String.fromUCS2(Native.label(raw, name));

	public function sum(data:haxe.io.Bytes):Int
		return Native.sum(raw, data, data.length);

	public function close():Void
		Native.close(raw);

	public static function live():Int
		return Native.live();

	public static function fail():Void
		Native.fail();

	public static function nap(milliseconds:Int):Void
		Native.nap(milliseconds);
}
