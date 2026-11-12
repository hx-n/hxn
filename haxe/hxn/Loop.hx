package hxn;

/**
	Controls when results from native libraries are delivered to the app.

	By default, each library's results are delivered from the Haxe event loop
	of the thread that first requested a delivery. This suits a program
	without its own main loop. An app with its own loop, such as a frame
	loop, sets `manual` and calls `sync` at the point where deliveries should
	happen. Results are then delivered only inside `sync`, and the Haxe event
	loop is neither used nor kept alive.

	In both modes a delivery is one batch: continuations run, stream items
	are delivered and signals are set in the order the library produced them,
	and effects rerun once at the end.
**/
class Loop {
	@:allow(hxn) static final inboxes:Array<Inbox> = [];

	/** True when the app calls `sync` itself. **/
	public static var manual = false;

	/**
		Delivers every library's ready results on the calling thread.
		Returns whether anything is still being watched.
	**/
	public static function sync():Bool {
		var watching = false;
		Reactive.batch(() -> {
			for (inbox in inboxes)
				if (inbox.sync())
					watching = true;
		});
		// Each inbox checks the meters in its sync. With no inbox, they are checked here.
		if (inboxes.length == 0)
			Gc.check();
		return watching;
	}

	/**
		Exits the program with `code`, like `Sys.exit`.

		Use it instead of `Sys.exit` in a program built with hxcpp. After a
		collection, hxcpp continues zeroing memory on its own background
		threads. During exit the C runtime destroys a lock those threads
		acquire, so a program that exits soon after a collection can crash on
		Windows instead of exiting. This affects hxcpp 4.3.2 and hxcpp from
		git before HaxeFoundation/hxcpp#1298, which fixed it. A library's memory can trigger a
		collection at a time the program did not choose, including just
		before it exits. This function flushes stdout and stderr and exits
		without running those destructors. On other targets it calls
		`Sys.exit`.
	**/
	public static function exit(code:Int):Void {
		#if (cpp && !emscripten)
		Sys.stdout().flush();
		Sys.stderr().flush();
		leave(code);
		#else
		Sys.exit(code);
		#end
	}

	#if (cpp && !emscripten)
	@:native("::_Exit") extern static function leave(code:Int):Void;
	#end
}
