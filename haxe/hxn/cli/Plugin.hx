package hxn.cli;

import haxe.Json;

/**
	`Plugin` is the main function of a tool that a haxelib registers with hxn,
	as `hxn.cli` in its `haxelib.json` describes. hxn sends the tool one
	request on standard input. For a command, such as `hxn me dev`, `Plugin`
	runs the command that the arguments select. For a hook, it calls `hook`
	and prints its reply:

	```haxe
	static function main()
		hxn.cli.Plugin.main(new hxn.cli.Cli("hxn me", "Creates and develops hxnme apps.", [new New(), new Dev()]), hook);
	```

	A `Failure` that a command or hook throws prints its message after the
	tool's name, on standard error, and ends the process with its status.
	Any other exception ends it with status 1.
**/
class Plugin {
	/** Answers the request on standard input with `cli` and `hook`, and ends the process with the status. **/
	public static function main(cli:Cli, ?hook:(phase:String, request:Dynamic) -> Null<Reply>):Void {
		var status = try answer(cli, hook, Sys.stdin().readAll().toString()) catch (failure:Failure) {
			Sys.stderr().writeString('${cli.program}: ${failure.message}\n');
			failure.status;
		} catch (error:haxe.Exception) {
			Sys.stderr().writeString('${cli.program}: ${error.message}\n');
			1;
		}
		Sys.exit(status);
	}

	/** Answers the request `text`, and returns the exit status. **/
	public static function answer(cli:Cli, hook:Null<(phase:String, request:Dynamic) -> Null<Reply>>, text:String):Int {
		var request:Dynamic = Json.parse(text);
		if (request.version != 1)
			throw new Failure('hxn sent a request of version ${request.version}, and this tool reads version 1');
		switch (request.kind : String) {
			case "command":
				var context = new Context(request.cwd, request.root, request.library, request);
				return cli.run(request.arguments, context);
			case "hook":
				var reply = hook == null ? null : hook(request.phase, request);
				Sys.println((reply == null ? new Reply() : reply).toJson());
				return 0;
			case kind:
				throw new Failure('hxn sent a request of the unknown kind $kind');
		}
	}
}
