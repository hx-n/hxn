/** The core Haxe logger also works without native bindings, on JS and eval. */
class LogFrontend {
	static function main():Void {
		var received:Array<hxn.LogRecord> = [];
		hxn.Log.sink = record -> received.push(record);
		hxn.Log.level = Info;
		hxn.Log.debug("hidden");
		hxn.Log.info("frontend naïve ☃");
		if (received.length != 1 || received[0].target != "LogFrontend" || received[0].line <= 0)
			throw "frontend filtering or position metadata failed";
		hxn.Log.info(StringTools.lpad("", "☃", 4000));
		if (!received[1].truncated || haxe.io.Bytes.ofString(received[1].message).length > 8192)
			throw "frontend UTF-8 truncation failed";
		final value = new LogValue();
		hxn.Log.debug(value);
		if (value.calls != 0)
			throw "disabled frontend log converted its argument";
		hxn.Log.info(value);
		if (value.calls != 1 || received[2].message != "converted")
			throw "enabled frontend log did not convert its argument once";
		#if js
		js.Lib.global.console.log("PASS: standalone Haxe logging");
		#else
		Sys.println("PASS: standalone Haxe logging");
		#end
	}
}

private class LogValue {
	public var calls = 0;

	public function new() {}

	public function toString():String {
		calls++;
		return "converted";
	}
}
