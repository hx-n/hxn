class Main {
	static function main() {
		var greeter = new Greeter("Hello");
		Sys.println(greeter.greet("Haxe"));
		Sys.println(greeter.greet("Rust"));
		Sys.println('${greeter.greeted()} greeted');
		try {
			greeter.greet("");
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
		greeter.close();
	}
}
