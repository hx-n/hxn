package hxn;

/** An owned diagnostic; timestamp is milliseconds since Unix epoch. */
typedef LogRecord = {
	final level:LogLevel;
	final target:String;
	final message:String;
	final file:String;
	final line:Int;
	final timestamp:Float;

	/** "rust" or "haxe". Targets identify the emitting module or subsystem. */
	final origin:String;

	final truncated:Bool;
}
