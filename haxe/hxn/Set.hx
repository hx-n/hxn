package hxn;

/**
	A mutable set of values exchanged with Rust.

	Array literals are accepted as sets. Construction copies the elements;
	`toArray` also returns a copy. Iteration preserves insertion order, or
	the order received from Rust. Adding an element does not sort the set.

	Membership uses Haxe `==` by default, including identity for objects.
	Pass an equality function when values need a different comparison.
	Rust applies its own `Eq` or `Ord` when it receives the set.
**/
@:forward(length, add, has, remove, clear, iterator, toArray, copy)
abstract Set<T>(SetValues<T>) {
	public inline function new(?values:Iterable<T>, ?equals:(T, T) -> Bool) {
		this = new SetValues(values, equals);
	}

	@:from public static function fromArray<T>(values:Array<T>):Set<T> {
		return values == null ? null : new Set(values);
	}
}

private class SetValues<T> {
	final values:Array<T> = [];
	final equals:(T, T) -> Bool;
	public var length(get, never):Int;

	public function new(?initial:Iterable<T>, ?equals:(T, T) -> Bool) {
		this.equals = equals == null ? (a, b) -> a == b : equals;
		if (initial != null)
			for (value in initial)
				add(value);
	}

	inline function get_length():Int
		return values.length;

	/** Adds a value if absent, returning whether the set changed. **/
	public function add(value:T):Bool {
		if (has(value))
			return false;
		values.push(value);
		return true;
	}

	public function has(value:T):Bool {
		for (item in values)
			if (equals(item, value))
				return true;
		return false;
	}

	/** Removes a value, returning whether it was present. **/
	public function remove(value:T):Bool {
		for (index in 0...values.length)
			if (equals(values[index], value)) {
				values.splice(index, 1);
				return true;
			}
		return false;
	}

	public function clear():Void
		values.resize(0);

	public inline function iterator():Iterator<T>
		return values.iterator();

	public inline function toArray():Array<T>
		return values.copy();

	public function copy():Set<T>
		return new Set(values, equals);
}
