package hxn.cli;

/**
	`Failure` ends a command with a message for the user. `Plugin` prints the
	message after the tool's name and ends the process with `status`.
**/
class Failure extends haxe.Exception {
	public final status:Int;

	public function new(message:String, status = 1) {
		super(message);
		this.status = status;
	}
}
