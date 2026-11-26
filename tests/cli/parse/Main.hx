import hxn.cli.Cli;
import hxn.cli.Command;
import hxn.cli.Context;

/** Runs a dev server. **/
class Serve implements Command {
	/** The program file. **/
	@:arg @:value("file") var file = "app.hxml";

	/** The Ash command. **/
	@:option @:env("HXN_CLI_TEST_ASH") @:value("command") var ash = "ash";

	/** The port. **/
	@:option("--port", "-p") var port = 8080;

	/** Whether the server runs threaded. **/
	@:option var threads = false;

	/** The files to watch. **/
	@:option("--watch") var watch:Array<String> = [];

	public var ran = "";

	public function run(context:Context):Int {
		ran = '$file $ash $port $threads ${watch.join(",")}';
		return 0;
	}
}

/** Builds with another compiler. **/
class Build implements Command {
	@:option var jit = false;
	@:arg @:raw var compiler:Array<String> = [];

	public function run(context:Context):Int
		return 0;
}

/** Copies files. **/
class Copy implements Command {
	@:arg @:choices("hl", "cpp") var runtime:String;
	@:arg var files:Array<String> = [];

	public var ran = "";

	public function run(context:Context):Int {
		ran = '$runtime ${files.join(",")}';
		return 0;
	}
}

/** Checks the CLI library's parser in Haxe's interpreter. **/
class Main {
	static var failed = 0;

	static function main() {
		var context = new Context(Sys.getCwd(), Sys.getCwd(), "test");
		var serve = new Serve(), copy = new Copy();
		var cli = new Cli("hxn test", "Tests the parser.", [serve, copy]);
		check("defaults", cli.run(["serve"], context) == 0 && serve.ran == "app.hxml ash 8080 false ");
		serve = new Serve();
		cli = new Cli("hxn test", "Tests the parser.", [serve, copy]);
		check("options before and after the argument, with = and a short form", cli.run(["serve", "--ash=a b", "x.hxml", "-p", "9", "--threads", "--watch", "w1", "--watch=w2"], context) == 0
			&& serve.ran == "x.hxml a b 9 true w1,w2");
		var mistakes = [
			["serve", "--port", "nine"] => "--port takes a whole number, not nine",
			["serve", "--port"] => "--port needs a value, <port>",
			["serve", "--nope"] => "unknown option --nope",
			["serve", "a", "b"] => "unexpected argument b",
			["serve", "--threads=yes"] => "--threads takes no value",
			["serve", "-p", "1", "-p", "2"] => "--port is given twice",
			["copy"] => "<runtime> is missing",
			["copy", "js"] => "<runtime> is one of hl, cpp, not js"
		];
		for (args => message in mistakes)
			check('mistake: ${args.join(" ")}', mistake(new Serve(), new Copy(), args) == message);
		copy = new Copy();
		Cli.assign(copy, ["cpp", "--", "-a", "b"]);
		check("-- ends the options", copy.ran == "" && @:privateAccess copy.files.join(",") == "-a,b" && @:privateAccess copy.runtime == "cpp");
		var build = new Build();
		Cli.assign(build, ["--jit", "-lib", "x", "--jit", "app.hxml"]);
		check("a raw argument takes the options of another program", @:privateAccess build.jit && @:privateAccess build.compiler.join(" ") == "-lib x --jit app.hxml");
		Sys.putEnv("HXN_CLI_TEST_ASH", "from-env");
		serve = new Serve();
		Cli.assign(serve, []);
		check("an environment variable gives an absent option", @:privateAccess serve.ash == "from-env");
		serve = new Serve();
		Cli.assign(serve, ["--ash", "given"]);
		check("the option wins over the environment variable", @:privateAccess serve.ash == "given");
		var help = cli.help(new Serve());
		check("help", help.indexOf("usage: hxn test serve [--ash <command>] [--port <port>] [--threads] [--watch <watch>] [<file>]") >= 0
			&& help.indexOf("-p, --port <port>") >= 0 && help.indexOf("HXN_CLI_TEST_ASH gives") >= 0
			&& help.indexOf("The default is app.hxml.") >= 0 && help.indexOf("Runs a dev server.") == 0);
		var overview = cli.overview();
		check("overview", overview.indexOf("serve  Runs a dev server.") >= 0 && overview.indexOf("copy   Copies files.") >= 0);
		// An empty variable gives no value.
		Sys.putEnv("HXN_CLI_TEST_ASH", "");
		serve = new Serve();
		cli = new Cli("hxn test", "Tests the parser.", [serve]);
		var request = haxe.Json.stringify({version: 1, kind: "command", library: "test", root: "/r", cwd: "/c", arguments: ["serve", "y.hxml"]});
		check("a command request runs the command", hxn.cli.Plugin.answer(cli, null, request) == 0 && serve.ran == "y.hxml ash 8080 false ");
		var reply = new hxn.cli.Reply();
		reply.haxeArgs.push(["-D", "ready"]);
		check("a reply has hxn's field names", reply.toJson() == '{"version":1,"haxe_args":[["-D","ready"]]}' && new hxn.cli.Reply().toJson() == '{"version":1}');
		var phases = [];
		hxn.cli.Plugin.answer(cli, (phase, _) -> {
			phases.push(phase);
			return null;
		}, '{"version":1,"kind":"hook","library":"test","root":"/r","phase":"before-build","context":{}}');
		check("a hook request calls the hook with its phase", phases.join(",") == "before-build");
		var refused = try {
			hxn.cli.Plugin.answer(cli, null, '{"version":2,"kind":"command"}');
			false;
		} catch (_:hxn.cli.Failure) true;
		check("another version of the protocol is refused", refused);
		if (failed > 0) {
			Sys.println('$failed checks failed');
			Sys.exit(1);
		}
		Sys.println("CLI parser checks passed");
	}

	static function mistake(serve:Serve, copy:Copy, args:Array<String>):String {
		try
			Cli.assign(args[0] == "serve" ? serve : copy, args.slice(1))
		catch (m:hxn.cli.Cli.Mistake)
			return m.message;
		return "no mistake";
	}

	static function check(name:String, ok:Bool) {
		if (!ok)
			failed++;
		Sys.println('$name: ${ok ? "yes" : "no"}');
	}
}
