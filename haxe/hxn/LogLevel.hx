package hxn;

/** Maximum verbosity, shared with hxn::log::Level. Off disables all records. */
enum abstract LogLevel(Int) to Int {
	var Off = 0;
	var Error = 1;
	var Warn = 2;
	var Info = 3;
	var Debug = 4;
	var Trace = 5;

	public function toString():String {
		return switch (this) {
			case 0: "OFF";
			case 1: "ERROR";
			case 2: "WARN";
			case 3: "INFO";
			case 4: "DEBUG";
			case _: "TRACE";
		};
	}
}
