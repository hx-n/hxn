class Main {
	static function main() {
		var parsed = Document.parse('{"name": "hxn", "tags": ["rust", "haxe"], "stars": 3, "nested": {"ok": true, "none": null}}');

		// The value is a recursive enum, so nested patterns work.
		switch (parsed) {
			case Fields(fields):
				var names = [for (name in fields.keys()) name];
				names.sort(Reflect.compare);
				Sys.println('fields: ${names.join(", ")}');
				switch (fields.get("tags")) {
					case Items([Text(first), _]):
						Sys.println('the first tag is $first');
					case _:
				}
			case _:
		}

		// Pass a value back to the library.
		Sys.println(describe(Document.at(parsed, "/tags/1")));
		Sys.println(describe(Document.at(parsed, "/nested")));
		Sys.println(describe(Document.at(parsed, "/nested/none")));
		Sys.println(describe(Document.at(parsed, "/missing")));

		// Serialize a value built in Haxe.
		Sys.println(Document.write(Items([Number(1.5), Text("two"), Nothing, Fields(["three" => Flag(true)])]), false));

		// A parse error is a typed exception that carries the position.
		try {
			Document.parse('{"name": "hxn",\n "tags": [1, 2,}');
		} catch (mistake:ParseException) {
			Sys.println('line ${mistake.error.line}, column ${mistake.error.column}: ${mistake.error.message}');
		}
	}

	static function describe(value:Null<Value>):String {
		return switch (value) {
			case null: "nothing there";
			case Nothing: "null";
			case Flag(on): 'the flag $on';
			case Number(number): 'the number $number';
			case Text(text): 'the text "$text"';
			case Items(items): 'a list of ${items.length}';
			case Fields(fields): 'an object of ${Lambda.count(fields)}';
		}
	}
}
