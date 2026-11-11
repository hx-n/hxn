package hxn;

import hxn.reactive.Node;

/**
	A value that can change, and that remembers who read it.

	Reading one inside an effect or a computed value subscribes that reader:
	it runs again when the signal is set to something different. Nothing is
	subscribed by hand.

	```haxe
	var count = new Signal(0);
	var doubled = new Computed(() -> count.value * 2);
	Reactive.effect(() -> trace(doubled.value));
	count.value = 2; // traces 4
	```
**/
class Signal<T> {
	@:allow(hxn) final node:Node;

	/** Reads and subscribes; assigning sets. **/
	public var value(get, set):T;

	/** `equals` decides whether a new value is a change. The default is `==`. **/
	public function new(initial:T, ?equals:(T, T) -> Bool)
		node = new Node(initial, null, false, cast equals);

	/** The value. The effect or computed value that is running will rerun when it changes. **/
	public inline function get():T
		return node.read();

	/** The value, without subscribing whoever asks. **/
	public inline function peek():T
		return node.value;

	/** Sets the value and reruns what depends on it, unless it is the same. **/
	public inline function set(next:T):Void
		node.write(next);

	/** Sets the value from the one it has. **/
	public inline function update(change:T->T):Void
		node.write(change(node.value));

	inline function get_value():T
		return node.read();

	inline function set_value(next:T):T {
		node.write(next);
		return next;
	}
}
