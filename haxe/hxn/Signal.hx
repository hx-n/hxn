package hxn;

import hxn.reactive.Node;

/**
	A mutable value that tracks its readers.

	Reading a signal inside an effect or a computed value subscribes that
	reader, which reruns when the signal is set to a different value.
	Subscriptions are automatic.

	```haxe
	var count = new Signal(0);
	var doubled = new Computed(() -> count.value * 2);
	Reactive.effect(() -> trace(doubled.value));
	count.value = 2; // traces 4
	```
**/
class Signal<T> {
	@:allow(hxn) final node:Node;

	/** Reading returns the value and subscribes the reader. Assigning sets the value. **/
	public var value(get, set):T;

	/** `equals` decides whether a new value is a change. The default is `==`. **/
	public function new(initial:T, ?equals:(T, T) -> Bool)
		node = new Node(initial, null, false, cast equals);

	/** Returns the value and subscribes the running effect or computed value, which reruns when the value changes. **/
	public inline function get():T
		return node.read();

	/** Returns the value without subscribing the reader. **/
	public inline function peek():T
		return node.value;

	/** Sets the value and reruns its dependents, unless the value is unchanged. **/
	public inline function set(next:T):Void
		node.write(next);

	/** Sets the value to the result of `change` applied to the current value. **/
	public inline function update(change:T->T):Void
		node.write(change(node.value));

	inline function get_value():T
		return node.read();

	inline function set_value(next:T):T {
		node.write(next);
		return next;
	}
}
