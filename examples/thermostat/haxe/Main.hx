class Main {
	static function main() {
		// This program controls when the library's results are delivered:
		// a frame loop would call `sync` once per frame. Without this line
		// they are delivered from the Haxe event loop.
		hxn.Loop.manual = true;

		var thermostat = new Thermostat(20);
		var target = thermostat.target();

		// Derived from the signal, and recomputed when it changes.
		var state = new hxn.Computed(() -> target.value > 21 ? "heating" : "idle");
		// Runs now, and again whenever anything it read has changed.
		hxn.Reactive.effect(() -> Sys.println('set to ${target.value}: ${state.value}'));

		thermostat.turn(1.5);
		hxn.Loop.sync();

		// Two changes before a delivery are delivered as one: a signal
		// reports only its latest value.
		thermostat.turn(1);
		thermostat.turn(-0.5);
		hxn.Loop.sync();

		// Nothing changed, so nothing runs.
		hxn.Loop.sync();

		try {
			thermostat.target();
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
		thermostat.close();
	}
}
