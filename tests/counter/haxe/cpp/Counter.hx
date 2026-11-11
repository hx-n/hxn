@:buildXml('
<files id="haxe">
	<compilerflag value="-I${HXN_INCLUDE}"/>
	<compilerflag value="-I${COUNTER_INCLUDE}"/>
</files>
<target id="haxe">
	<lib name="${COUNTER_LIB}"/>
	<lib name="-lpthread" if="linux"/>
	<lib name="-ldl" if="linux"/>
</target>
')
@:headerCode('
#include "counter_abi.h"

// Each call returns before its failure is thrown, and frees what it was given.
namespace counter_glue {

inline void *make(int start) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_new(start, &out, &err), err);
	return out;
}

inline int add(void *self, int amount) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_add((hxn_handle *)self, amount, &out, &err), err);
	return out;
}

inline ::String label(void *self, ::String name) {
	hx::strbuf storage;
	hxn_buf out = HXN_BUF_EMPTY;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_label((hxn_handle *)self, hxn::slice(name, storage), &out, &err), err);
	return hxn::take_string(out);
}

inline int sum(void *self, Array<unsigned char> data) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_sum((hxn_handle *)self, hxn::slice(data), &out, &err), err);
	return out;
}

inline void close(void *self) {
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_close((hxn_handle *)self, &err), err);
}

inline void fail() {
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_fail(&err), err);
}

inline void nap(int milliseconds) {
	hxn::Blocking zone;
	counter_nap(milliseconds);
}

}
')
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
}
