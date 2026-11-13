class Main {
	static function main() {
		var pantry = new Pantry();

		// A record is constructed from named fields. `note` is omitted.
		pantry.stock({name: "eggs", quantity: 12, unit: Unit.Each});
		pantry.stock({name: "flour", quantity: 2, unit: Unit.Kilo, note: "wholemeal"});
		var rice:Item = {name: "rice", quantity: 1, unit: Unit.Kilo};
		Sys.println('${pantry.stock(rice)} items stocked');

		// It was passed by copy: changing this object does not change the library's copy.
		rice.quantity = 100;
		Sys.println('rice: ${pantry.find("rice").quantity}');

		// A missing optional return value is null, and so is an omitted optional field.
		var flour = pantry.find("flour");
		Sys.println('flour: ${flour.quantity}, ${flour.note}');
		Sys.println('eggs note: ${pantry.find("eggs").note}, salt: ${pantry.find("salt")}');

		// Returns a list of records, and has an enum parameter.
		var low = pantry.low(Unit.Kilo, 5);
		Sys.println('low on ${[for (item in low) item.name].join(" and ")}');

		// An enum stays typed when it is used as a map key.
		var missing = pantry.shortages([Unit.Each => 15, Unit.Kilo => 5, Unit.Litre => 1]);
		Sys.println('short by unit: ${missing.get(Unit.Each)} each, ${missing.get(Unit.Kilo)} kilo, ${missing.get(Unit.Litre)} litre');

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
