private typedef Raw = hl.Abstract<"hxn_counter">;
private typedef RawFuture = hl.Abstract<"hxn_future">;
private typedef RawPromise = hl.Abstract<"hxn_promise">;

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

	public static function square_later(value:Int, milliseconds:Int):RawFuture
		return null;

	public static function greet_later(name:String, milliseconds:Int):RawFuture
		return null;

	public static function abandoned():RawFuture
		return null;

	public static function future_state(self:RawFuture):Int
		return 0;

	public static function future_wait(self:RawFuture, milliseconds:Int):Int
		return 0;

	public static function future_take_int(self:RawFuture):Int
		return 0;

	public static function future_take_text(self:RawFuture):hl.Bytes
		return null;

	public static function promise_new():RawPromise
		return null;

	public static function promise_future(self:RawPromise):RawFuture
		return null;

	public static function promise_resolve(self:RawPromise, value:Int):Bool
		return false;
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

	public static function squareLater(value:Int, milliseconds:Int):hxn.Future<Int>
		return future(Native.square_later(value, milliseconds), raw -> Native.future_take_int(raw));

	public static function greetLater(name:String, milliseconds:Int):hxn.Future<String>
		return future(Native.greet_later(name, milliseconds), raw -> @:privateAccess String.fromUCS2(Native.future_take_text(raw)));

	public static function abandoned():hxn.Future<Int>
		return future(Native.abandoned(), raw -> Native.future_take_int(raw));

	public static function future<T>(raw:RawFuture, take:RawFuture->T):hxn.Future<T>
		return new hxn.Future(() -> Native.future_state(raw), limit -> Native.future_wait(raw, limit), () -> take(raw));
}

/** A future the Haxe side settles. **/
class Promise {
	final raw:RawPromise;

	public function new()
		raw = Native.promise_new();

	public function future():hxn.Future<Int>
		return Counter.future(Native.promise_future(raw), raw -> Native.future_take_int(raw));

	public function resolve(value:Int):Bool
		return Native.promise_resolve(raw, value);
}
