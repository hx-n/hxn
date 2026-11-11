@:buildXml('
<files id="haxe">
	<compilerflag value="-I${HXN_INCLUDE}"/>
	<compilerflag value="-I${COUNTER_INCLUDE}"/>
	<compilerflag value="-I${COUNTER_INCLUDE}/../haxe/cpp"/>
</files>
<section if="emscripten">
	<compiler id="emscripten" exe="emcc">
		<flag value="-fexceptions"/>
	</compiler>
</section>
<target id="haxe">
	<lib name="${COUNTER_LIB}"/>
	<lib name="-lpthread" if="linux"/>
	<lib name="-ldl" if="linux"/>
</target>
')
@:headerInclude("counter_glue.h")
class Counter {
	@:unreflective var raw:cpp.RawPointer<cpp.Void>;

	public function new(start:Int) {
		raw = untyped __cpp__("counter_glue::make({0})", start);
		cpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));
	}

	// Runs inside a collection: it may free, and may not touch the Haxe heap.
	static function finalize(counter:Counter):Void {
		untyped __cpp__("counter_release((hxn_handle *){0})", counter.raw);
	}

	public function add(amount:Int):Int
		return untyped __cpp__("counter_glue::add({0}, {1})", raw, amount);

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
		return hxn.Callback.during(() -> untyped __cpp__("counter_glue::apply({0}, {1}, {2})", raw, times, guarded));
	}

	public function label(name:String):String
		return untyped __cpp__("counter_glue::label({0}, {1})", raw, name);

	public function sum(data:haxe.io.Bytes):Int
		return untyped __cpp__("counter_glue::sum({0}, {1})", raw, data.getData());

	public function close():Void
		untyped __cpp__("counter_glue::close({0})", raw);

	public static function live():Int
		return untyped __cpp__("counter_live()");

	public static function fail():Void
		untyped __cpp__("counter_glue::fail()");

	public static function nap(milliseconds:Int):Void
		untyped __cpp__("counter_glue::nap({0})", milliseconds);

	public static function squareLater(value:Int, milliseconds:Int):hxn.Future<Int> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::square_later({0}, {1})", value, milliseconds);
		return future.typed(future.takeInt);
	}

	public static function greetLater(name:String, milliseconds:Int):hxn.Future<String> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::greet_later({0}, {1})", name, milliseconds);
		return future.typed(future.takeText);
	}

	public static function sumLater(limit:Int):hxn.Future<Int> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::sum_later({0})", limit);
		return future.typed(future.takeInt);
	}

	public static function combinedLater(value:Int, limit:Int):hxn.Future<Int> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::combined_later({0}, {1})", value, limit);
		return future.typed(future.takeInt);
	}

	public static function countUp(limit:Int, fail = false):hxn.Stream<Int> {
		var stream = new StreamHandle();
		stream.raw = untyped __cpp__("counter_glue::count_up({0}, {1})", limit, fail);
		return stream.ofInt();
	}

	public static function abandoned():hxn.Future<Int> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::abandoned()");
		return future.typed(future.takeInt);
	}
}

/** Owns a native future; `hxn.Future` holds it through its three calls. **/
@:headerInclude("counter_glue.h")
private class FutureHandle {
	@:unreflective public var raw:cpp.RawPointer<cpp.Void>;

	public function new() {
		raw = null;
		cpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));
	}

	static function finalize(future:FutureHandle):Void {
		untyped __cpp__("counter_release((hxn_handle *){0})", future.raw);
	}

	public function typed<T>(take:() -> T):hxn.Future<T>
		return new hxn.Future(state, wait, take);

	function state():Int
		return untyped __cpp__("counter_glue::future_state({0})", raw);

	function wait(milliseconds:Int):Int
		return untyped __cpp__("counter_glue::future_wait({0}, {1})", raw, milliseconds);

	public function takeInt():Int
		return untyped __cpp__("counter_glue::future_take_int({0})", raw);

	public function takeText():String
		return untyped __cpp__("counter_glue::future_take_text({0})", raw);
}

/** A future the Haxe side settles. **/
@:headerInclude("counter_glue.h")
class Promise {
	@:unreflective var raw:cpp.RawPointer<cpp.Void>;

	public function new() {
		raw = untyped __cpp__("counter_glue::promise_new()");
		cpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));
	}

	static function finalize(promise:Promise):Void {
		untyped __cpp__("counter_release((hxn_handle *){0})", promise.raw);
	}

	public function future():hxn.Future<Int> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::promise_future({0})", raw);
		return future.typed(future.takeInt);
	}

	public function doubled():hxn.Future<Int> {
		var future = new FutureHandle();
		future.raw = untyped __cpp__("counter_glue::promise_doubled({0})", raw);
		return future.typed(future.takeInt);
	}

	public function resolve(value:Int):Bool
		return untyped __cpp__("counter_glue::promise_resolve({0}, {1})", raw, value);
}

/** A stream the Haxe side feeds. **/
@:headerInclude("counter_glue.h")
class Emitter {
	@:unreflective var raw:cpp.RawPointer<cpp.Void>;

	public function new() {
		raw = untyped __cpp__("counter_glue::emitter_new()");
		cpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));
	}

	static function finalize(emitter:Emitter):Void {
		untyped __cpp__("counter_release((hxn_handle *){0})", emitter.raw);
	}

	public function stream():hxn.Stream<Int> {
		var stream = new StreamHandle();
		stream.raw = untyped __cpp__("counter_glue::emitter_stream({0})", raw);
		return stream.ofInt();
	}

	public function emit(value:Int):Bool
		return untyped __cpp__("counter_glue::emitter_emit({0}, {1})", raw, value);

	public function end():Bool
		return untyped __cpp__("counter_glue::emitter_end({0})", raw);
}

/** Owns a native stream; `hxn.Stream` holds it through its two calls. **/
@:headerInclude("counter_glue.h")
private class StreamHandle {
	@:unreflective public var raw:cpp.RawPointer<cpp.Void>;

	public function new() {
		raw = null;
		cpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));
	}

	static function finalize(stream:StreamHandle):Void {
		untyped __cpp__("counter_release((hxn_handle *){0})", stream.raw);
	}

	public function ofInt():hxn.Stream<Int>
		return new hxn.Stream(wait, nextInt);

	function wait(milliseconds:Int):Int
		return untyped __cpp__("counter_glue::stream_wait({0}, {1})", raw, milliseconds);

	function nextInt():Int
		return untyped __cpp__("counter_glue::stream_next_int({0})", raw);
}
