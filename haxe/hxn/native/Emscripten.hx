package hxn.native;

#if (cpp && emscripten)
import haxe.Exception;
import sys.thread.EventLoop.NextEventTime;

/**
	Runs Haxe code from the host's event loop: the browser's, or Node's. A
	program in a page that spins on the Haxe event loop never lets the page
	run its own callbacks, such as a timer or a resolved promise, so a future
	they settle would stay pending; work scheduled here runs between them.

	The Haxe event loop stops once `main` has returned with nothing queued,
	so each turn here also runs the Haxe events that are due, and schedules
	another turn for those queued later.
**/
@:cppFileCode("#include <emscripten.h>")
class Emscripten {
	static var due:Array<() -> Void> = [];

	/** Runs `f` once, after the host has run whatever else is waiting. **/
	public static function later(f:() -> Void):Void {
		due.push(f);
		if (due.length == 1)
			wake(0);
	}

	static function wake(ms:Int):Void {
		untyped __cpp__("emscripten_async_call([](void *) { hx::NativeAttach attach; ::hxn::native::Emscripten_obj::run(); }, nullptr, {0})", ms);
	}

	@:keep static function run():Void {
		var now = due;
		due = [];
		var failure:Null<Exception> = null;
		for (f in now)
			try
				f()
			catch (error:Exception)
				if (failure == null)
					failure = error;
		var next:NextEventTime = Never;
		try
			next = sys.thread.Thread.current().events.progress()
		catch (error:Exception)
			if (failure == null)
				failure = error;
		switch next {
			case Now:
				wake(0);
			case At(time):
				wake(Std.int(Math.max(0, (time - haxe.Timer.stamp()) * 1000)));
			case Never | AnyTime(_):
		}
		if (failure != null)
			throw failure;
	}
}
#end
