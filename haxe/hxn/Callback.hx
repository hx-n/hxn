package hxn;

import haxe.Exception;

/**
	What a binding needs to lend a Haxe function to native code.

	Native code cannot be thrown through, so a binding wraps the function it
	passes: the wrapper catches, hands the exception to `failed`, and returns
	the value that tells the library its callback failed. The library stops
	and returns, and `during`, which the binding made the native call inside,
	throws the function's own exception to the caller.
**/
class Callback {
	static final pending = new sys.thread.Tls<Exception>();

	/** Keeps what a callback threw. The first of a call is the one reported. **/
	public static function failed(error:Exception):Void {
		if (pending.value == null)
			pending.value = error;
	}

	/**
		Runs `call`, a native call that was lent callbacks, and returns its
		result. If a callback threw, that is thrown here in place of whatever
		the library made of it.
	**/
	public static function during<T>(call:() -> T):T {
		var result:T;
		try {
			result = call();
		} catch (error:Exception) {
			rethrow();
			throw error;
		}
		rethrow();
		return result;
	}

	static function rethrow():Void {
		var error = pending.value;
		if (error != null) {
			pending.value = null;
			throw error;
		}
	}
}
