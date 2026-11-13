package hxn;

#if macro
import haxe.macro.Context;
import haxe.macro.Expr;
import haxe.macro.Type;

using haxe.macro.TypeTools;

/** Builds one array-backed tuple abstract per arity, shared by all element types. **/
class TupleMacro {
	public static function build():ComplexType {
		var parts = switch (Context.getLocalType()) {
			case TInst(_, parts): parts;
			case _: Context.error("expected hxn.Tuple", Context.currentPos());
		};
		if (parts.length == 0) {
			var values = Context.getCallArguments();
			if (values != null)
				Context.error("use hxn.Tuple.of(...) to infer the element types, or supply them to new hxn.Tuple<...>(...)", Context.currentPos());
			return TPath({pack: ["hxn"], name: "Tuple", sub: "TupleFactory"});
		}
		var made = path(parts.length);
		made.params = [for (part in parts) TPType(part.toComplexType())];
		return TPath({pack: ["hxn"], name: "Tuple", sub: "TupleValue", params: [TPType(TPath(made))]});
	}

	public static function construct(values:Array<Expr>):Expr {
		var value = {expr: ENew(path(values.length), values), pos: Context.currentPos()};
		return macro hxn.Tuple.TupleValue.wrap($value);
	}

	public static function access(self:Expr, index:Expr):Expr {
		var position = literalIndex(index);
		var type = Context.follow(Context.typeof(self));
		switch (type) {
			case TAbstract(definition, [inner]) if (definition.get().name == "TupleValue" && definition.get().module == "hxn.Tuple"):
				type = Context.follow(inner);
			case _:
		}
		var arity = switch (type) {
			case TAbstract(definition, parts) if (definition.get().pack.join(".") == "hxn.tuple"): parts.length;
			case _: Context.error("expected a concrete hxn.Tuple type", self.pos);
		};
		if (position < 0 || position >= arity)
			Context.error('tuple index $position is outside 0...$arity', index.pos);
		return {expr: EField(self, '_$position'), pos: index.pos};
	}

	static function literalIndex(index:Expr):Int {
		return switch (index.expr) {
			case EConst(CInt(text)): Std.parseInt(text);
			case EParenthesis(value): literalIndex(value);
			case EUnop(OpNeg, false, value): -literalIndex(value);
			case _: Context.error("a typed tuple position must be an integer literal; use [] for a runtime index", index.pos);
		};
	}

	static function path(arity:Int):TypePath {
		var pos = Context.currentPos();
		if (arity == 0)
			Context.error("a tuple needs at least one value", pos);
		var name = 'Tuple$arity';
		var made:TypePath = {pack: ["hxn", "tuple"], name: name};
		try {
			Context.getType('hxn.tuple.$name');
			return made;
		} catch (_:Dynamic) {}

		var parameters = [for (index in 0...arity) {name: 'T$index'}];
		var parameterTypes = [for (index in 0...arity) TPath({pack: [], name: 'T$index'})];
		var fields:Array<Field> = [];
		for (index in 0...arity) {
			var field = '_$index';
			fields.push({name: field, access: [APublic], kind: FProp("get", "set", parameterTypes[index]), pos: pos});
			fields.push({name: 'get_$field', access: [AInline], pos: pos,
				kind: FFun({args: [], ret: parameterTypes[index], expr: macro return cast this[$v{index}]})});
			fields.push({name: 'set_$field', access: [AInline], pos: pos,
				kind: FFun({args: [{name: "value", type: parameterTypes[index]}], ret: parameterTypes[index],
					expr: macro { this[$v{index}] = value; return value; }})});
		}
		var arguments = [for (index in 0...arity) macro $i{'_$index'}];
		fields.push({name: "new", access: [APublic, AInline], pos: pos,
			kind: FFun({args: [for (index in 0...arity) {name: '_$index', type: parameterTypes[index]}],
				expr: macro this = ($a{arguments}:Array<Dynamic>)})});
		fields.push({name: "length", access: [APublic], kind: FProp("get", "never", macro:Int), pos: pos});
		fields.push({name: "get_length", access: [AInline],
			kind: FFun({args: [], ret: macro:Int, expr: macro return $v{arity}}), pos: pos});

		var own:TypePath = {pack: made.pack, name: name, params: [for (part in parameterTypes) TPType(part)]};
		var record = ComplexType.TAnonymous([for (index in 0...arity) {
			name: '_$index', kind: FieldType.FVar(parameterTypes[index]), pos: pos
		}]);
		var values = [for (index in 0...arity) {expr: EField(macro value, '_$index'), pos: pos}];
		fields.push({name: "fromRecord", access: [APublic, AStatic, AInline], pos: pos,
			meta: [{name: ":from", pos: pos}],
			kind: FFun({params: parameters, args: [{name: "value", type: record}], ret: TPath(own),
				expr: macro { if (value == null) return cast null; return cast ($a{values}:Array<Dynamic>); }})});
		Context.defineType({pack: made.pack, name: name, pos: pos,
			params: parameters, kind: TDAbstract(macro:Array<Dynamic>), fields: fields});
		return made;
	}
}
#end
