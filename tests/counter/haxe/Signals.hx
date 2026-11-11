import hxn.Computed;
import hxn.Reactive;
import hxn.Signal;

/** Checks for the reactive core, which is Haxe alone and so the same on every runtime. **/
class Signals {
	public static function run(check:(what:String, ok:Bool) -> Void) {
		var count = new Signal(1);
		var seen = [];
		var stop = Reactive.effect(() -> seen.push(count.value));
		check("an effect runs when it is made", seen.join(",") == "1");
		count.value = 2;
		count.set(2);
		count.update(value -> value + 1);
		check('and again for each change, not for the same value: $seen', seen.join(",") == "1,2,3");
		stop();
		count.value = 9;
		check("a stopped effect stays stopped", seen.length == 3 && count.peek() == 9);

		// A computed value is worked out when read, and kept.
		var a = new Signal(2);
		var computes = 0;
		var squared = new Computed(() -> {
			computes++;
			return a.value * a.value;
		});
		check("a computed value is not worked out until read", computes == 0);
		check("then it is, once", squared.value == 4 && squared.value == 4 && computes == 1);
		a.value = 3;
		check("a change alone does not work it out again", computes == 1);
		check("reading it does", squared.value == 9 && computes == 2);

		// Two paths to one value: computed once, never from old and new mixed.
		var base = new Signal(1);
		var plusOne = new Computed(() -> base.value + 1);
		var timesTwo = new Computed(() -> base.value * 2);
		var sums = [];
		Reactive.effect(() -> sums.push(plusOne.value + timesTwo.value));
		base.value = 2;
		base.value = 10;
		check('a value reached by two paths is computed once a change: $sums', sums.join(",") == "4,7,31");

		// A computed value that comes out the same stops the change there.
		var number = new Signal(2);
		var parity = new Computed(() -> number.value % 2);
		var parities = 0;
		Reactive.effect(() -> {
			parity.value;
			parities++;
		});
		number.value = 4;
		number.value = 6;
		check("an unchanged computed value reruns nothing", parities == 1);
		number.value = 7;
		check("a changed one does", parities == 2);

		// Dependencies follow the branch taken.
		var useFirst = new Signal(true);
		var first = new Signal("a");
		var second = new Signal("b");
		var picked = [];
		Reactive.effect(() -> picked.push(useFirst.value ? first.value : second.value));
		second.value = "b2";
		check("a signal on the branch not taken is not a dependency", picked.join(",") == "a");
		useFirst.value = false;
		first.value = "a2";
		second.value = "b3";
		check('and becomes one when the branch changes: $picked', picked.join(",") == "a,b2,b3");

		// Batching and untracking.
		var width = new Signal(1);
		var height = new Signal(1);
		var areas = [];
		Reactive.effect(() -> areas.push(width.value * height.value));
		Reactive.batch(() -> {
			width.value = 3;
			height.value = 4;
		});
		check('a batch of sets is one rerun: $areas', areas.join(",") == "1,12");
		var trigger = new Signal(0);
		var ignored = new Signal(0);
		var untracked = [];
		Reactive.effect(() -> untracked.push(trigger.value + Reactive.untrack(() -> ignored.value)));
		ignored.value = 5;
		trigger.value = 1;
		check('an untracked read is not a dependency: $untracked', untracked.join(",") == "0,6");

		// Cleanups, and effects made by effects.
		var topic = new Signal("x");
		var log = [];
		var stopOuter = Reactive.effect(() -> {
			var name = topic.value;
			log.push('open $name');
			Reactive.onCleanup(() -> log.push('close $name'));
		});
		topic.value = "y";
		stopOuter();
		check('a cleanup runs before a rerun and on stopping: $log', log.join(",") == "open x,close x,open y,close y");

		var outer = new Signal(0);
		var inner = new Signal(0);
		var innerRuns = 0;
		Reactive.effect(() -> {
			outer.value;
			Reactive.effect(() -> {
				inner.value;
				innerRuns++;
			});
		});
		outer.value = 1;
		outer.value = 2;
		inner.value = 1;
		check('an effect made by an effect is replaced with it, not added to: $innerRuns', innerRuns == 4);

		// A root owns what is made inside it.
		var rooted = new Signal(0);
		var rootedRuns = 0;
		var dispose = Reactive.root(dispose -> {
			Reactive.effect(() -> {
				rooted.value;
				rootedRuns++;
			});
			return dispose;
		});
		rooted.value = 1;
		dispose();
		rooted.value = 2;
		check("disposing a root stops what it owns", rootedRuns == 2);

		// An effect may set a signal; what depends on that settles too.
		var source = new Signal(1);
		var mirror = new Signal(0);
		var mirrored = [];
		Reactive.effect(() -> mirror.value = source.value * 10);
		Reactive.effect(() -> mirrored.push(mirror.value));
		source.value = 2;
		check('an effect that sets a signal reruns what reads it: $mirrored', mirrored.join(",") == "10,20");

		// What counts as a change can be chosen.
		var point = new Signal({x: 1, y: 2}, (old, next) -> old.x == next.x && old.y == next.y);
		var moves = 0;
		Reactive.effect(() -> {
			point.value;
			moves++;
		});
		point.value = {x: 1, y: 2};
		point.value = {x: 5, y: 2};
		check("a signal's own equality decides what is a change", moves == 2);

		// A failing effect leaves the graph usable.
		var risky = new Signal(0);
		var survived = 0;
		Reactive.effect(() -> {
			if (risky.value == 1)
				throw "refused";
			survived++;
		});
		var thrown = false;
		try {
			risky.value = 1;
		} catch (error:haxe.Exception) {
			thrown = true;
		}
		risky.value = 2;
		check("an effect that throws is thrown from the set, and runs again later", thrown && survived == 2);
	}
}
