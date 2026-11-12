package hxn;

import hxn.reactive.Node;

/**
	A value derived from signals and other computed values, and cached until
	one of them changes. SolidJS calls this a memo.

	It is computed on first read. It is recomputed only when a dependency has
	changed and either it is read again or an effect depends on it. If the
	new value equals the old one, its dependents do not rerun.
**/
class Computed<T> {
	final node:Node;

	/** Reads the value and subscribes the reader, computing it first if needed. **/
	public var value(get, never):T;

	public function new(compute:() -> T, ?equals:(T, T) -> Bool)
		node = new Node(null, cast compute, false, cast equals);

	public inline function get():T
		return node.read();

	/** Unsubscribes from its dependencies. A computed value created inside an effect is disposed with that effect. **/
	public inline function dispose():Void
		node.dispose();

	inline function get_value():T
		return node.read();
}
