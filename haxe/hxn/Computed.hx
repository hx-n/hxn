package hxn;

import hxn.reactive.Node;

/**
	A value worked out from signals and other computed values, and kept until
	one of them changes. What SolidJS calls a memo.

	It is computed when first read, and again only when something it read has
	changed and it is read again or an effect depends on it. If the new value
	is the same as the old, what depends on it does not rerun.
**/
class Computed<T> {
	final node:Node;

	/** Reads and subscribes, computing first if needed. **/
	public var value(get, never):T;

	public function new(compute:() -> T, ?equals:(T, T) -> Bool)
		node = new Node(null, cast compute, false, cast equals);

	public inline function get():T
		return node.read();

	/** Stops following its sources. One made inside an effect is disposed with it. **/
	public inline function dispose():Void
		node.dispose();

	inline function get_value():T
		return node.read();
}
