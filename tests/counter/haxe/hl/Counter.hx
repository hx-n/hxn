private typedef Raw = hl.Abstract<"hxn_counter">;
private typedef RawFuture = hl.Abstract<"hxn_future">;
private typedef RawPromise = hl.Abstract<"hxn_promise">;
private typedef RawStream = hl.Abstract<"hxn_stream">;
private typedef RawEmitter = hl.Abstract<"hxn_emitter">;

@:hlNative("counter")
private class Native {
	public static function make(start:Int):Raw
		return null;

	public static function add(self:Raw, amount:Int):Int
		return 0;

	public static function apply(self:Raw, times:Int, f:Int->Null<Int>):Int
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

	public static function sum_later(limit:Int):RawFuture
		return null;

	public static function combined_later(value:Int, limit:Int):RawFuture
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

	public static function promise_doubled(self:RawPromise):RawFuture
		return null;

	public static function promise_resolve(self:RawPromise, value:Int):Bool
		return false;

	public static function count_up(limit:Int, fail:Bool):RawStream
		return null;

	public static function stream_wait(self:RawStream, milliseconds:Int):Int
		return 0;

	public static function stream_next_int(self:RawStream):Int
		return 0;

	public static function emitter_new():RawEmitter
		return null;

	public static function emitter_stream(self:RawEmitter):RawStream
		return null;

	public static function emitter_emit(self:RawEmitter, value:Int):Bool
		return false;

	public static function emitter_end(self:RawEmitter):Bool
		return false;
}

class Counter {
	final raw:Raw;

	public function new(start:Int)
		raw = Native.make(start);

	public function add(amount:Int):Int
		return Native.add(raw, amount);

	public function apply(times:Int, f:Int->Int):Int {
		// Native code cannot be thrown through: keep the exception, tell the
		// library its callback failed, and let `during` throw it afterwards.
		var guarded = function(value:Int):Null<Int> {
			try {
				return f(value);
			} catch (error:haxe.Exception) {
				hxn.Callback.failed(error);
				return null;
			}
		};
		return hxn.Callback.during(() -> Native.apply(raw, times, guarded));
	}

	// A function used by later calls is kept here, where the collector can
	// see it, and lent to each of them. Native code holds nothing, so one
	// that refers back to this counter is collected with it.
	var step:Null<Int->Int>;

	public function setStep(f:Int->Int):Void
		step = f;

	public function advance(times:Int):Int {
		var step = step;
		if (step == null)
			throw new haxe.Exception("no step set");
		return apply(times, step);
	}

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

	public static function sumLater(limit:Int):hxn.Future<Int>
		return future(Native.sum_later(limit), raw -> Native.future_take_int(raw));

	public static function combinedLater(value:Int, limit:Int):hxn.Future<Int>
		return future(Native.combined_later(value, limit), raw -> Native.future_take_int(raw));

	public static function countUp(limit:Int, fail = false):hxn.Stream<Int>
		return streamOfInt(Native.count_up(limit, fail));

	public static function streamOfInt(raw:RawStream):hxn.Stream<Int>
		return new hxn.Stream(limit -> Native.stream_wait(raw, limit), () -> Native.stream_next_int(raw));

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

	public function doubled():hxn.Future<Int>
		return Counter.future(Native.promise_doubled(raw), raw -> Native.future_take_int(raw));

	public function resolve(value:Int):Bool
		return Native.promise_resolve(raw, value);
}

/** A stream the Haxe side feeds. **/
class Emitter {
	final raw:RawEmitter;

	public function new()
		raw = Native.emitter_new();

	public function stream():hxn.Stream<Int>
		return Counter.streamOfInt(Native.emitter_stream(raw));

	public function emit(value:Int):Bool
		return Native.emitter_emit(raw, value);

	public function end():Bool
		return Native.emitter_end(raw);
}
