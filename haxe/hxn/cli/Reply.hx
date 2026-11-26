package hxn.cli;

import haxe.Json;

/** `Reply` is what a hook gives hxn back: arguments for the compiler and for Ash, and files for the web page. **/
class Reply {
	/** The Haxe flags, each with its value: `-D`, `-cp` or `--macro`. **/
	public final haxeArgs:Array<Array<String>> = [];

	/** The Ash options, each a flag with or without its value. **/
	public final ashArgs:Array<Array<String>> = [];

	/** The files to copy next to the web page. **/
	public final pageFiles:Array<String> = [];

	public function new() {}

	/** Returns the reply as the JSON that hxn reads. **/
	public function toJson():String {
		var reply:Dynamic = {version: 1};
		if (haxeArgs.length > 0)
			reply.haxe_args = haxeArgs;
		if (ashArgs.length > 0)
			reply.ash_args = ashArgs;
		if (pageFiles.length > 0)
			reply.page_files = pageFiles;
		return Json.stringify(reply);
	}
}
