/** Namespace-only consumer, also compiled with full DCE and two native libraries. */
class LogOnly {
	static function main():Void {
		var received:Array<hxn.LogRecord> = [];
		hxn.Log.sink = record -> received.push(record);
		hxn.Log.level = Debug;
		#if hxn_log_second
		final greeter = new Greeter("Hello");
		greeter.greet("logging");
		greeter.close();
		#end
		Diagnostics.emitLogs(1);
		if (received.length != 2 || received[0].level != Debug || received[1].message != "native naïve ☃ 0")
			throw "namespace logging registration lost or records delivered twice";
		hxn.Log.info("frontend", "fixture");
		if (received.length != 3 || received[2].origin != "haxe")
			throw "frontend does not share the sink";
		if (Math.abs(received[0].timestamp - received[2].timestamp) >= 60000)
			throw "native and frontend clocks do not share Unix milliseconds";
		#if hxn_log_worker
		if (Diagnostics.logWork().await() != 42 || received.length != 4 || received[3].message != "worker record")
			throw "native worker does not share the sink";
		#end
		#if hxn_log_console
		hxn.Log.sink = null;
		var probe = "platform console";
		#if hxn_log_probe
		probe += " " + LogProbe.id;
		#end
		hxn.Log.warn(probe, "fixture");
		#end
		Sys.println("PASS: namespace logging, DCE and shared sink");
		hxn.Loop.exit(0);
	}
}
