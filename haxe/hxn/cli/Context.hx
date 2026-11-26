package hxn.cli;

/** `Context` is what hxn tells a command about where it runs. **/
class Context {
	/** The directory that the user ran the command in. **/
	public final cwd:String;

	/** The directory of the installed package that provides the command. **/
	public final root:String;

	/** The package's library, as its registration names it. **/
	public final library:String;

	/** The whole request that hxn sent. **/
	public final request:Dynamic;

	public function new(cwd:String, root:String, library:String, ?request:Dynamic) {
		this.cwd = cwd;
		this.root = root;
		this.library = library;
		this.request = request;
	}
}
