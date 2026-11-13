package hxn;

#if !macro
/**
	A fixed group of typed values, such as `Tuple<String, Int>`.

	Construct one with `Tuple.of("name", 42)` or
	`new Tuple<String, Int>("name", 42)`. `pair[index]` reads or writes a
	position as `Dynamic`, with bounds checked at runtime. `pair.at(1)` and
	`pair.set(1, value)` use a literal position and keep its static type.
	The typed `_0`, `_1`, ... fields are also available.
	`copy` makes a shallow copy and `toArray`
	returns a separate `Array<Dynamic>` for inspecting mixed values.

	Record literals with the numbered fields are also accepted. A tuple
	has at least one position, and its `length` is fixed.
**/
@:genericBuild(hxn.TupleMacro.build())
class Tuple<Rest> {}
#end

/** Adds indexing to the concrete array-backed tuple without another allocation. **/
@:noCompletion
@:forward
@:forward.new
@:transitive
abstract TupleValue<T>(T) from T {
	@:arrayAccess public inline function get(index:Int):Dynamic {
		var values:Array<Dynamic> = cast this;
		if (index < 0 || index >= values.length)
			throw new haxe.Exception("tuple index out of range: " + index);
		return values[index];
	}

	@:arrayAccess public inline function put(index:Int, value:Dynamic):Dynamic {
		var values:Array<Dynamic> = cast this;
		if (index < 0 || index >= values.length)
			throw new haxe.Exception("tuple index out of range: " + index);
		return values[index] = value;
	}

	public inline function copy():TupleValue<T> {
		var values:Array<Dynamic> = cast this;
		return cast values.copy();
	}

	public inline function toArray():Array<Dynamic> {
		var values:Array<Dynamic> = cast this;
		return values.copy();
	}

	public static inline function wrap<T>(value:T):TupleValue<T>
		return cast value;

	public macro function at(self:haxe.macro.Expr, index:haxe.macro.Expr):haxe.macro.Expr {
		return hxn.TupleMacro.access(self, index);
	}

	public macro function set(self:haxe.macro.Expr, index:haxe.macro.Expr, value:haxe.macro.Expr):haxe.macro.Expr {
		var field = hxn.TupleMacro.access(self, index);
		return macro $field = $value;
	}
}

/** The constructor available as `hxn.Tuple.of`. **/
@:noCompletion class TupleFactory {
	public static macro function of(values:Array<haxe.macro.Expr>):haxe.macro.Expr {
		return hxn.TupleMacro.construct(values);
	}
}
