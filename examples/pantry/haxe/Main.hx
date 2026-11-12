class Main {
	static function main() {
		var pantry = new Pantry();

		// A record is made from its fields by name, and `note` left out.
		pantry.stock({name: "eggs", quantity: 12, unit: Unit.Each});
		pantry.stock({name: "flour", quantity: 2, unit: Unit.Kilo, note: "wholemeal"});
		var rice:Item = {name: "rice", quantity: 1, unit: Unit.Kilo};
		Sys.println('${pantry.stock(rice)} items stocked');

		// It crossed by copy: changing this one changes nothing in the library.
		rice.quantity = 100;
		Sys.println('rice: ${pantry.find("rice").quantity}');

		// One that may be missing is null, and so is a field that was left out.
		var flour = pantry.find("flour");
		Sys.println('flour: ${flour.quantity}, ${flour.note}');
		Sys.println('eggs note: ${pantry.find("eggs").note}, salt: ${pantry.find("salt")}');

		// A list of records, and an enum going in.
		var low = pantry.low(Unit.Kilo, 5);
		Sys.println('low on ${[for (item in low) item.name].join(" and ")}');

		try {
			pantry.stock({name: "air", quantity: 0, unit: Unit.Litre});
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}

		// A typed error: the exception carries the enum value and the
		// library's message.
		Sys.println('eggs left: ${pantry.take("eggs", 2).quantity}');
		for (wanted in ["eggs", "salt"]) {
			try {
				pantry.take(wanted, 50);
			} catch (refused:PantryException) {
				switch (refused.error) {
					case Short(name, left):
						Sys.println('only $left of $name');
					case Unknown(name):
						Sys.println('no $name at all');
				}
				Sys.println('  ${refused.message}');
			}
		}
		pantry.close();
	}
}
