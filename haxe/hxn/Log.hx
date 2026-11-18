package hxn;

import haxe.PosInfos;
import haxe.io.Bytes;
import haxe.io.BytesInput;
#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

using haxe.macro.ExprTools;
#end

/**
	Shared logging for Haxe and Rust libraries. Set level and sink on the app
	thread. By default Rust records are pulled after native calls and during Loop.sync;
	flush explicitly when an otherwise idle native worker can produce logs.
	No worker calls a Haxe sink. Logging does not keep an idle app alive.
 */
#if (cpp && android)
@:cppFileCode("#include <android/log.h>")
#end
class Log {
	static final sources:Array<{level:Int->Void, take:() -> Null<Bytes>}> = [];
	static var queued:Array<LogRecord> = [];
	static var dropped = 0.;
	static var flushing = false;
	static var threshold = 3;
	#if target.threaded
	static final lock = new sys.thread.Mutex();
	static final owner = sys.thread.Thread.current();
	#end

	/** Maximum verbosity for both frontends and attached native libraries. */
	public static var level(get, set):LogLevel;

	/** Called on the logging thread. Exceptions are contained; null restores console output. */
	public static var sink:Null<LogRecord->Void>;

	static function get_level():LogLevel {
		#if target.threaded
		lock.acquire();
		#end
		final result:LogLevel = cast threshold;
		#if target.threaded
		lock.release();
		#end
		return result;
	}

	static function set_level(value:LogLevel):LogLevel {
		if ((value : Int) < 0 || (value : Int) > 5)
			throw new haxe.Exception("invalid log level");
		#if target.threaded
		lock.acquire();
		#end
		threshold = value;
		#if target.threaded
		lock.release();
		#end
		for (source in sources)
			source.level(value);
		return value;
	}

	/** Checks verbosity before preparing an expensive Haxe message. */
	public static inline function enabled(value:LogLevel):Bool
		return (value : Int) > 0 && (value : Int) <= (level : Int);

	public static inline function error(message:Dynamic, ?target:String, ?pos:PosInfos):Void
		write(Error, message, target, pos);

	public static inline function warn(message:Dynamic, ?target:String, ?pos:PosInfos):Void
		write(Warn, message, target, pos);

	public static inline function info(message:Dynamic, ?target:String, ?pos:PosInfos):Void
		write(Info, message, target, pos);

	public static inline function debug(message:Dynamic, ?target:String, ?pos:PosInfos):Void
		write(Debug, message, target, pos);

	public static inline function trace(message:Dynamic, ?target:String, ?pos:PosInfos):Void
		write(Trace, message, target, pos);

	/** Emits an owned record; disabled messages are not converted to text. */
	public static function write(value:LogLevel, message:Dynamic, ?target:String, ?pos:PosInfos):Void {
		if (!enabled(value))
			return;
		final text = bound(Std.string(message), 8192);
		final scope = bound(target == null ? (pos == null ? "haxe" : pos.className) : target, 256);
		final file = bound(pos == null ? "" : pos.fileName, 1024);
		final record:LogRecord = {
			level: value,
			message: text.value,
			target: scope.value,
			file: file.value,
			line: pos == null ? 0 : pos.lineNumber,
			timestamp: now(),
			origin: "haxe",
			truncated: text.truncated || scope.truncated || file.truncated};
		// A sink that logs must not recursively call itself or create an endless queue.
		#if target.threaded
		if (sys.thread.Thread.current() == owner && flushing) {
		#else
		if (flushing) {
		#end
			console(record);
			return;
		}

		#if target.threaded
		lock.acquire();
		#end
		if (queued.length < 256)
			queued.push(record);
		else
			dropped++;
		#if target.threaded
		lock.release();
		#end
		boundary();
	}

	static function bound(value:String, limit:Int):{value:String, truncated:Bool} {
		final bytes = Bytes.ofString(value.substr(0, limit));
		var end = bytes.length < limit ? bytes.length : limit;
		while (end > 0 && end < bytes.length && (bytes.get(end) & 192) == 128)
			end--;
		return {value: bytes.getString(0, end), truncated: value.length > limit || bytes.length > end};
	}

	/** Used by generated bindings; older bindings need no logging exports. */
	@:noCompletion public static function attach(setLevel:Int->Void, take:() -> Null<Bytes>):Bool {
		setLevel(level);
		sources.push({level: setLevel, take: take});
		return true;
	}

	/** Delivers one bounded batch per native source on the app's logging thread. */
	public static function flush():Void {
		#if target.threaded
		if (!(sys.thread.Thread.current() == owner))
			return;
		#end
		if (flushing)
			return;
		flushing = true;
		try {
			#if target.threaded
			lock.acquire();
			#end
			final batch = queued.length == 0 ? null : queued, lost = dropped;
			if (batch != null)
				queued = [];
			dropped = 0;
			#if target.threaded
			lock.release();
			#end
			if (batch != null)
				for (record in batch)
					deliver(record);
			loss(lost, "haxe");
			final count = sources.length;
			for (i in 0...count) {
				final source = sources[i];
				final bytes = source.take();
				if (bytes == null || bytes.length == 0)
					continue;
				final input = new BytesInput(bytes);
				final lost = input.readDouble(), count = input.readInt32();
				if (count < 0 || count > 256)
					throw new haxe.Exception("invalid native log batch");
				for (_ in 0...count) {
					final severity = input.readInt32();
					if (severity < 1 || severity > 5)
						throw new haxe.Exception("invalid native log level");
					final timestamp = input.readDouble(), target = text(input, 256), message = text(input, 8192), file = text(input, 1024);
					final line = input.readInt32(), truncated = input.readByte() != 0;
					deliver({
						level: cast severity,
						target: target,
						message: message,
						file: file,
						line: line,
						timestamp: timestamp,
						origin: "rust",
						truncated: truncated
					});
				}
				if (input.position != bytes.length)
					throw new haxe.Exception("trailing native log bytes");
				loss(lost, "rust");
			}
		} catch (error:Dynamic) {
			// Diagnostics must not replace a successful result or a library's exception.
			console({
				level: Error,
				target: "hxn.log",
				message: 'Log delivery failed: $error',
				file: "",
				line: 0,
				timestamp: now(),
				origin: "haxe",
				truncated: false
			});
		}
		flushing = false;
	}

	static function text(input:BytesInput, limit:Int):String {
		final length = input.readInt32();
		if (length < 0 || length > limit || length > input.length - input.position)
			throw new haxe.Exception("invalid native log text");
		return input.readString(length);
	}

	static function loss(count:Float, origin:String):Void {
		if (count > 0)
			deliver({
				level: Warn,
				target: "hxn.log",
				message: 'Dropped $count log records (queue full)',
				file: "",
				line: 0,
				timestamp: now(),
				origin: origin,
				truncated: false
			});
	}

	static function deliver(record:LogRecord):Void {
		if (!enabled(record.level))
			return;
		final handler = sink;
		if (handler == null) {
			console(record);
			return;
		}
		try
			handler(record)
		catch (_:Dynamic)
			console(record);
	}

	static function console(record:LogRecord):Void {
		final message = '[${record.level.toString()}] ${record.target}: ${record.message}' + (record.truncated ? " [truncated]" : "");
		try {
			#if js
			switch (record.level) {
				case Error:
					js.Lib.global.console.error(message);
				case Warn:
					js.Lib.global.console.warn(message);
				case Debug | Trace:
					js.Lib.global.console.debug(message);
				case _:
					js.Lib.global.console.info(message);
			}
			#elseif (cpp && android)
			final priority = 7 - (record.level : Int);
			untyped __cpp__("__android_log_write({0}, \"hxn\", {1}.utf8_str())", priority, message);
			#else
			Sys.stderr().writeString(message + "\n");
			#end
		} catch (_:Dynamic) {}
	}

	/** Flushes after a native call without allocating a callback around the call. */
	@:noCompletion public static inline function after<T>(value:T):T {
		boundary();
		return value;
	}

	/** A frame loop batches diagnostics at Loop.sync instead of polling after every call. */
	@:noCompletion public static inline function boundary():Void {
		#if (hl || cpp)
		if (Loop.manual)
			return;
		#end
		flush();
	}

	static inline function now():Float {
		#if sys
		return Sys.time() * 1000.;
		#else
		return Date.now().getTime();
		#end
	}

	#if macro
	/** Flushes only after the binding has converted results and restored callback/exception state. */
	public static function bind():Array<Field> {
		final fields = Context.getBuildFields();
		function returns(expr:Expr):Expr {
			return switch (expr.expr) {
				case EFunction(_, _): expr;
				case EReturn(value) if (value != null): macro return hxn.Log.after($value);
				case EReturn(_): macro {hxn.Log.boundary(); return;};
				case _: expr.map(returns);
			};
		}
		for (field in fields) {
			if (StringTools.startsWith(field.name, "hxn"))
				continue;
			switch (field.kind) {
				case FFun(fun) if (fun.expr != null):
					final body = returns(fun.expr);
					fun.expr = macro {
						try {
							$body;
							hxn.Log.boundary();
						} catch (__hxn_log_error:haxe.Exception) {
							hxn.Log.boundary();
							throw __hxn_log_error;
						}
					};
				case _:
			}
		}
		return fields;
	}
	#end
}
