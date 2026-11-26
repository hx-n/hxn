package hxn.cli;

/** `Spec` describes a command, as `CommandMacro` reads it from the command's class. **/
typedef Spec = {
	var name:String;
	var doc:String;
	var arguments:Array<ArgumentSpec>;
	var options:Array<OptionSpec>;
}

/** `ArgumentSpec` describes a positional argument. **/
typedef ArgumentSpec = {
	var field:String;
	var value:String;
	var doc:String;
	var kind:Kind;
	var required:Bool;
	var fallback:Null<String>;
	var choices:Array<String>;
}

/** `OptionSpec` describes an option. **/
typedef OptionSpec = {
	var field:String;
	var long:String;
	var short:Null<String>;
	var value:String;
	var doc:String;
	var kind:Kind;
	var env:Null<String>;
	var fallback:Null<String>;
}

/** `Kind` is the type of the value that an argument or option takes. **/
enum abstract Kind(String) to String {
	var Text = "text";
	var Integer = "int";
	var Number = "float";
	var Flag = "flag";
	var List = "list";
	var Rest = "rest";
	var Raw = "raw";
}
