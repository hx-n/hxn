package hxn;

import haxe.Exception;

/**
	Support for a binding that passes a Haxe function to native code as a
	callback.

	An exception cannot unwind through native code, so a binding wraps the
	function it passes. The wrapper catches the exception, passes it to
	`failed`, and returns the value that reports a failed callback to the
	library. The library stops and returns. The binding makes the native call
	inside `during`, which then rethrows the original exception to the caller.
**/
class Callback {
	static final pending = new sys.thread.Tls<Exception>();

	/** Records the exception a callback threw. Only the first one in a native call is reported. **/
	public static function failed(error:Exception):Void {
		if (pending.value == null)
			pending.value = error;
	}

	/**
		Runs `call`, a native call that was passed callbacks, and returns its
		result. If a callback threw, its exception is rethrown here and
		replaces the result or error the library returned.
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
