package hxn;

/**
	When what native libraries have produced reaches the app.

	Left alone, each library's results are delivered from the Haxe event loop
	of the thread that first asked for one, which suits a program that has no
	loop of its own. An app that has one, a frame loop say, sets `manual` and
	calls `sync` where it wants deliveries to happen. Nothing then arrives at
	any other time, and the Haxe event loop is neither used nor kept alive.

	Either way a delivery is one batch: continuations run, stream items are
	handed over and signals are set in the order the library produced them,
	and effects rerun once at the end.
**/
class Loop {
	@:allow(hxn) static final inboxes:Array<Inbox> = [];

	/** True when the app calls `sync` itself. **/
	public static var manual = false;

	/**
		Delivers everything every library has ready, on the calling thread.
		Returns whether anything is still being waited for.
	**/
	public static function sync():Bool {
		var watching = false;
		Reactive.batch(() -> {
			for (inbox in inboxes)
				if (inbox.sync())
					watching = true;
		});
		// A delivery checks the meters; with no library delivering, this does.
		if (inboxes.length == 0)
			Gc.check();
		return watching;
	}
}
