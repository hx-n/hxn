/** How the fixture's programs end. **/
class Finish {
	/**
		Ends the program with `code`.

		After a collection hxcpp goes on zeroing memory on threads of its own,
		and `exit` destroys the lock they take when they are done, which on
		Windows is a crash. A program that ends right after a collection, as
		these do, leaves without running destructors.
	**/
	public static function exit(code:Int):Void {
		#if (cpp && !emscripten)
		Sys.stdout().flush();
		untyped __cpp__("::_Exit({0})", code);
		#else
		Sys.exit(code);
		#end
	}
}
