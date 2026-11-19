import hxn.Future;
import hxn.Inbox;
import hxn.Loop;

/** Checks shared queues and independently allocated notification IDs. */
class SharedInbox {
	public static function run(check:(String, Bool) -> Void):Void {
		final manual = Loop.manual;
		Loop.manual = true;
		final queue:Array<Int> = [];
		final first = new Inbox(_ -> queue.length > 0, () -> queue.length == 0 ? -1 : queue.shift());
		final second = new Inbox(_ -> queue.length > 0, () -> queue.length == 0 ? -1 : queue.shift());
		var ready = false;
		var one = 0;
		var two = 0;
		new Future<Int>(first, () -> 11, () -> ready ? 1 : 0, _ -> ready ? 1 : 0, () -> 1).then(value -> one = value);
		new Future<Int>(second, () -> 22, () -> ready ? 1 : 0, _ -> ready ? 1 : 0, () -> 2).then(value -> two = value);
		first.sync();
		second.sync();
		ready = true;
		queue.push(11);
		queue.push(22);
		first.sync();
		second.sync();
		check("shared outbox reaches both libraries", one == 1 && two == 2);

		var firstReady = false;
		var secondReady = false;
		var delivered = 0;
		new Future<Int>(first, () -> 33, () -> firstReady ? 1 : 0, _ -> firstReady ? 1 : 0, () -> 3).then(_ -> delivered++);
		new Future<Int>(second, () -> 33, () -> secondReady ? 1 : 0, _ -> secondReady ? 1 : 0, () -> 4).then(_ -> delivered++);
		first.sync();
		second.sync();
		firstReady = true;
		queue.push(33);
		first.sync();
		second.sync();
		check("matching IDs still check their own readiness", delivered == 1);
		secondReady = true;
		queue.push(33);
		second.sync();
		check("matching IDs each deliver once", delivered == 2);

		var fail = false;
		var continued = false;
		first.watch(44, () -> {
			if (fail)
				throw new haxe.Exception("expected inbox failure");
			return true;
		});
		second.watch(44, () -> {
			if (!fail)
				return true;
			continued = true;
			return false;
		});
		first.sync();
		second.sync();
		fail = true;
		queue.push(44);
		var caught = false;
		try
			first.sync()
		catch (_:haxe.Exception)
			caught = true;
		second.sync();
		check("watcher failure still delivers other libraries", caught && continued);
		@:privateAccess Loop.inboxes.remove(first);
		@:privateAccess Loop.inboxes.remove(second);
		Loop.manual = manual;
	}

	static function main():Void {
		run((message, valid) -> {
			if (!valid)
				throw new haxe.Exception(message);
		});
		Sys.println("PASS: shared inbox notifications");
	}
}
