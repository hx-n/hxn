@:buildXml('
<files id="haxe">
	<compilerflag value="-I${HXN_INCLUDE}"/>
	<compilerflag value="-I${COUNTER_INCLUDE}"/>
	<compilerflag value="-I${COUNTER_INCLUDE}/../haxe/cpp"/>
</files>
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
