/**
	Work that throws after `main` returns. Work scheduled with it still runs,
	and then the program ends with status 1.
**/
class Throws {
	static function main() {
		hxn.native.Emscripten.later(() -> throw "broken turn");
		hxn.native.Emscripten.later(() -> Sys.println("the next turn ran"));
	}
}
