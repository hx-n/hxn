import haxe.Json;
import sys.io.File;

/** The CLI integration fixture's installed extension. **/
class Tool {
	static function main() {
		var request:Dynamic = Json.parse(Sys.stdin().readAll().toString());
		if (request.version != 1)
			throw "wrong protocol";
		if (request.kind == "command") {
			var args:Array<String> = request.arguments;
			Sys.println(args.join("|"));
			if (args.indexOf("fail") >= 0)
				Sys.exit(7);
			return;
		}
		var phase:String = request.phase;
		var file = File.append(Sys.getEnv("HXN_PLUGIN_LOG"));
		file.writeString(phase + "\n");
		file.close();
		if (Sys.getEnv("HXN_PLUGIN_FAIL") == phase)
			Sys.exit(9);
		if (phase == "after-build") {
			Sys.println(Json.stringify({
				version: 1,
				haxe_args: [["-D", "plugin_fixture=ready"]],
				page_files: [request.root + "/sources/plugin-agent.mjs"]
			}));
		} else
			Sys.println('{"version":1}');
	}
}
