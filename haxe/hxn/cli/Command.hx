package hxn.cli;

/**
	`Command` is a command of a tool, such as `dev` in `hxn me dev`. The class
	declares its arguments and options as fields, and `run` does the work:

	```haxe
	// The doc comments of the class and its fields are the help.
	class Dev implements hxn.cli.Command {
		@:arg var file = "app.hxml";

		@:option @:env("HXN_ASH") @:value("command") var ash = "ash";

		public function run(context:hxn.cli.Context):Int { ... }
	}
	```

	A macro reads the fields at compile time and generates `spec`, which `Cli`
	parses the arguments with. The class's doc comment is the command's help,
	and its first sentence is the summary in the list of commands.

	`@:arg` marks a positional argument. Arguments take their values in the
	order of their fields. Each one is required unless it has an initializer,
	has a `Null` type, or is an `Array<String>`, which takes the rest of the
	arguments. `@:raw` on that array also takes the arguments after the first
	one that is not an option of the command, including those that start with
	`-`, such as the arguments of another program. `@:choices("hl", "cpp")`
	limits the values.

	`@:option` marks an option. Its name is `--` and the field's name, or the
	name that `@:option("--name", "-n")` gives, with an optional short form.
	A `Bool` field is a flag, an `Array<String>` field takes each value of a
	repeated option, and other fields take one value of their type, `String`,
	`Int` or `Float`. `@:value("name")` gives the name of the value in the
	help, and `@:env("NAME")` an environment variable that gives the value when
	the option is absent.

	The macro adds a constructor that takes no arguments if the class has
	none, so that fields can have initializers.

	Build macros do not run when a macro types a class. If a macro of the
	tool also types the class that lists the commands, put that list in
	`#if !macro`.
**/
@:autoBuild(hxn.cli.CommandMacro.build())
interface Command {
	/** Does the command's work after its fields have their values. Returns the exit status. **/
	function run(context:Context):Int;

	/** Returns what the macro read from the class. **/
	function spec():Spec;
}
