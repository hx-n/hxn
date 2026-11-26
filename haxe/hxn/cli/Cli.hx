package hxn.cli;

import hxn.cli.Spec;

using StringTools;

/**
	`Cli` runs one of a tool's commands with the arguments that the user
	gave. It picks the command by the first argument, gives the command's
	fields their values from the rest, and calls its `run`. `--help` or `-h`
	prints the help of the tool or of a command. A mistake in the arguments
	prints the mistake and the command's usage to standard error, and ends
	with status 2.

	Options take their value as the next argument or after `=`, as in
	`--ash ash` or `--ash=ash`, and can come before, between or after the
	positional arguments. `--` ends the options: the arguments after it are
	positional.
**/
class Cli {
	/** The name that the user types for the tool, such as `hxn me`. **/
	public final program:String;

	/** What the tool does, for its help. **/
	public final doc:String;

	/** The commands of the tool. **/
	public final commands:Array<Command>;

	public function new(program:String, doc:String, commands:Array<Command>) {
		this.program = program;
		this.doc = doc;
		this.commands = commands;
	}

	/** Runs the command that `arguments` select in `context`, and returns the exit status. **/
	public function run(arguments:Array<String>, context:Context):Int {
		var first = arguments[0];
		if (first == null || first == "help" || first == "--help" || first == "-h") {
			var asked = first == "help" ? find(arguments[1]) : null;
			Sys.print(asked != null ? help(asked) : overview());
			return 0;
		}
		var command = find(first);
		if (command == null) {
			Sys.stderr().writeString('error: $program has no command `$first`\n\n${overview()}');
			return 2;
		}
		var rest = arguments.slice(1);
		if (rest.contains("--help") || (rest.contains("-h") && !rest.slice(0, rest.indexOf("-h")).contains("--"))) {
			Sys.print(help(command));
			return 0;
		}
		try
			assign(command, rest)
		catch (mistake:Mistake) {
			Sys.stderr().writeString('error: ${mistake.message}\n\nusage: ${usage(command)}\n\nRun `$program ${command.spec().name} --help` for more.\n');
			return 2;
		}
		return command.run(context);
	}

	function find(name:Null<String>):Null<Command> {
		for (command in commands)
			if (command.spec().name == name)
				return command;
		return null;
	}

	/** Gives the fields of `command` their values from `arguments`. It throws `Mistake` for arguments that do not fit. **/
	public static function assign(command:Command, arguments:Array<String>):Void {
		var spec = command.spec();
		var positional:Array<String> = [];
		var given = new Map<String, Bool>();
		var lists = new Map<String, Array<String>>();
		var raw = Lambda.exists(spec.arguments, a -> a.kind == Raw);
		var i = 0;
		var options = true;
		while (i < arguments.length) {
			var word = arguments[i++];
			if (options && word == "--") {
				options = false;
				continue;
			}
			if (!options || !word.startsWith("-") || word == "-") {
				positional.push(word);
				if (raw && positional.length >= spec.arguments.length) {
					// The raw argument takes everything after its first word.
					for (j in i...arguments.length)
						positional.push(arguments[j]);
					break;
				}
				continue;
			}
			var equals = word.startsWith("--") ? word.indexOf("=") : -1;
			var name = equals < 0 ? word : word.substr(0, equals);
			var option = Lambda.find(spec.options, o -> o.long == name || o.short == name);
			if (option == null) {
				if (raw && positional.length >= spec.arguments.length - 1) {
					// An option of another program starts the raw argument.
					for (j in i - 1...arguments.length)
						positional.push(arguments[j]);
					break;
				}
				throw new Mistake('unknown option $name');
			}
			if (option.kind == Flag) {
				if (equals >= 0)
					throw new Mistake('${option.long} takes no value');
				Reflect.setProperty(command, option.field, true);
				given[option.field] = true;
				continue;
			}
			var value = if (equals >= 0) word.substr(equals + 1) else if (i < arguments.length) arguments[i++] else
				throw new Mistake('${option.long} needs a value, <${option.value}>');
			if (option.kind == List) {
				if (!lists.exists(option.field))
					lists[option.field] = [];
				lists[option.field].push(value);
			} else {
				if (given.exists(option.field))
					throw new Mistake('${option.long} is given twice');
				Reflect.setProperty(command, option.field, convert(value, option.kind, option.long));
			}
			given[option.field] = true;
		}
		for (field => values in lists)
			Reflect.setProperty(command, field, values);
		for (option in spec.options)
			if (!given.exists(option.field) && option.env != null) {
				var value = Sys.getEnv(option.env);
				if (value != null && value != "")
					Reflect.setProperty(command, option.field,
						option.kind == Flag ? value != "0" && value != "false" : option.kind == List ? [value] : convert(value, option.kind, option.env));
			}
		var at = 0;
		for (argument in spec.arguments) {
			switch argument.kind {
				case Rest | Raw:
					Reflect.setProperty(command, argument.field, positional.slice(at));
					at = positional.length;
				case _ if (at < positional.length):
					var value = positional[at++];
					if (argument.choices.length > 0 && !argument.choices.contains(value))
						throw new Mistake('<${argument.value}> is one of ${argument.choices.join(", ")}, not $value');
					Reflect.setProperty(command, argument.field, convert(value, argument.kind, '<${argument.value}>'));
				case _ if (argument.required):
					throw new Mistake('<${argument.value}> is missing');
				case _:
			}
		}
		if (at < positional.length)
			throw new Mistake('unexpected argument ${positional[at]}');
	}

	static function convert(value:String, kind:Kind, name:String):Dynamic {
		return switch kind {
			case Integer:
				if (!~/^-?[0-9]+$/.match(value))
					throw new Mistake('$name takes a whole number, not $value');
				Std.parseInt(value);
			case Number:
				var number = Std.parseFloat(value);
				if (Math.isNaN(number))
					throw new Mistake('$name takes a number, not $value');
				number;
			case _:
				value;
		}
	}

	/** Returns the usage line of `command`, without the program's name. **/
	public function usage(command:Command):String {
		var spec = command.spec();
		var words = [program, spec.name];
		for (option in spec.options)
			if (option.kind == Flag)
				words.push('[${option.long}]');
			else
				words.push('[${option.long} <${option.value}>]');
		for (argument in spec.arguments)
			words.push(switch argument.kind {
				case Rest | Raw: '[<${argument.value}>...]';
				case _ if (argument.required): '<${argument.value}>';
				case _: '[<${argument.value}>]';
			});
		return words.join(" ");
	}

	/** Returns the help of `command`: its doc comment, its usage, and its arguments and options. **/
	public function help(command:Command):String {
		var spec = command.spec();
		var out = new StringBuf();
		if (spec.doc != "")
			out.add(wrap(spec.doc, "") + "\n\n");
		out.add('usage: ${usage(command)}\n');
		var rows:Array<Array<String>> = [];
		for (argument in spec.arguments) {
			var notes = [argument.doc];
			if (argument.choices.length > 0)
				notes.push('One of ${argument.choices.join(", ")}.');
			if (argument.fallback != null)
				notes.push('The default is ${argument.fallback}.');
			rows.push(['<${argument.value}>', notes.filter(n -> n != "").join(" ")]);
		}
		if (rows.length > 0)
			out.add("\nArguments:\n" + table(rows));
		rows = [];
		for (option in spec.options) {
			var names = (option.short != null ? option.short + ", " : "") + option.long + (option.kind == Flag ? "" : ' <${option.value}>');
			var notes = [option.doc];
			if (option.fallback != null && option.kind != Flag)
				notes.push('The default is ${option.fallback}.');
			if (option.env != null)
				notes.push('${option.env} gives it when the option is absent.');
			rows.push([names, notes.filter(n -> n != "").join(" ")]);
		}
		rows.push(["-h, --help", "Prints this help."]);
		out.add("\nOptions:\n" + table(rows));
		return out.toString();
	}

	/** Returns the help of the tool: what it does, and a summary of each command. **/
	public function overview():String {
		var out = new StringBuf();
		if (doc != "")
			out.add(wrap(doc, "") + "\n\n");
		out.add('usage: $program <command> [<arguments>...]\n\nCommands:\n');
		out.add(table([for (command in commands) [command.spec().name, summary(command.spec().doc)]]));
		out.add('\nRun `$program <command> --help` for the help of a command.\n');
		return out.toString();
	}

	// Returns the first sentence of `doc`.
	static function summary(doc:String):String {
		var end = doc.indexOf(". ");
		var first = end < 0 ? doc.split("\n")[0] : doc.substr(0, end + 1);
		return first;
	}

	// Returns the rows with their first column padded to one width, and the second wrapped after it.
	static function table(rows:Array<Array<String>>):String {
		var width = 0;
		for (row in rows)
			if (row[0].length > width)
				width = row[0].length;
		var out = new StringBuf();
		for (row in rows) {
			var indent = "".rpad(" ", width + 4);
			var text = wrap(row[1], indent);
			out.add("  " + row[0].rpad(" ", width) + "  " + text.substr(indent.length) + "\n");
		}
		return out.toString();
	}

	// Returns `text` wrapped at 80 columns, with `indent` before each line.
	static function wrap(text:String, indent:String):String {
		var lines:Array<String> = [];
		for (paragraph in text.split("\n\n")) {
			var line = indent;
			for (word in paragraph.split(" ")) {
				if (line.length > indent.length && line.length + 1 + word.length > 80) {
					lines.push(line);
					line = indent;
				}
				line += (line.length > indent.length ? " " : "") + word;
			}
			lines.push(line);
			lines.push("");
		}
		lines.pop();
		return lines.join("\n");
	}
}

/** `Mistake` is an argument that does not fit the command. `Cli` prints it with the command's usage. **/
class Mistake extends haxe.Exception {}
