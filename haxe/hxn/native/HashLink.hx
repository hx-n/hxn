package hxn.native;

#if macro
import haxe.macro.Context;
import haxe.macro.Expr;
import hxn.Native;

// The HashLink implementation: a primitive for each declaration, and the
// declaration as a function that converts its arguments and result around it.
@:noCompletion class HashLink {
	final library:String;
	final extra:Array<Field> = [];
	var cells = false;

	public function new(library:String)
		this.library = library;

	function primitive(name:String, full:String, args:Array<FunctionArg>, ret:ComplexType):Field {
		return Native.method(name, args, ret, Native.nothing(Native.kind(ret)), [
			{name: ":hlNative", params: [macro $v{library}, macro $v{full}], pos: Context.currentPos()}
		], [AStatic]);
	}

	public function implement(declared:Array<Field>):Array<Field> {
		var out = [];
		for (declaration in declared) {
			var f = Native.functionOf(declaration);
			var given = Native.given(declaration);
			var args:Array<FunctionArg> = [];
			var passed:Array<Expr> = [];
			for (arg in f.args)
				argument(declaration, given, arg, args, passed);
			var name = declaration.name + "Primitive";
			var body = result(declaration, Native.returnType(f), name, args, passed);
			out.push(primitive(name, library + "_" + declaration.name, args, primitiveResult(declaration, f)));
			out.push(Native.method(declaration.name, f.args, Native.returnType(f), body, [], [APublic, AStatic]));
		}
		if (cells)
			extra.push(primitive("hxnAtomicCell", library + "_atomic_cell", [{name: "self", type: Native.type("hxn.Handle")}],
				Native.type("hl.Ref<Int>")));
		return out.concat(extra);
	}

	function argument(declaration:Field, given:Map<String, String>, arg:FunctionArg, args:Array<FunctionArg>,
			passed:Array<Expr>) {
		var name = arg.name;
		switch Native.kind(arg.type) {
			case "String" | "Bool" | "Int" | "Float":
				args.push({name: name, type: arg.type});
				passed.push(macro $i{name});
			case "haxe.Int64":
				args.push({name: name, type: Native.type("hl.I64")});
				passed.push(macro $i{name});
			case "haxe.io.Bytes":
				args.push({name: name, type: Native.type("hl.Bytes")});
				args.push({name: name + "Length", type: Native.type("Int")});
				passed.push(macro $i{name});
				passed.push(macro $i{name}.length);
			case "hxn.Handle":
				args.push({name: name, type: arg.type});
				passed.push(macro $i{name});
			case "hxn.AtomicInt" | "hxn.AtomicBool":
				args.push({name: name, type: Native.type("hl.Ref<Int>")});
				passed.push(macro $i{name} == null ? null : $i{name}.pointer);
			case "function":
				var lent = lent(declaration, given, name, arg.type);
				args.push({name: name, type: lent.type});
				passed.push(lent.wrapper);
			case other:
				Native.unsupported(declaration, "HashLink", 'parameter of type $other');
		}
	}

	function primitiveResult(declaration:Field, f:Function):ComplexType {
		return switch Native.kind(f.ret) {
			case "Void" | "Bool" | "Int" | "Float": Native.returnType(f);
			case "haxe.Int64": Native.type("hl.I64");
			case "String" | "haxe.io.Bytes": Native.type("hl.Bytes");
			case "hxn.Handle" | "hxn.AtomicInt": Native.type("hxn.Handle");
			case other: Native.unsupported(declaration, "HashLink", 'result of type $other');
		}
	}

	function result(declaration:Field, ret:ComplexType, name:String, args:Array<FunctionArg>,
			passed:Array<Expr>):Expr {
		var made = Native.call("", name, passed);
		return switch Native.kind(ret) {
			case "Void": made;
			case "Bool" | "Int" | "Float" | "haxe.Int64": macro return $made;
			case "String": macro return @:privateAccess String.fromUCS2($made);
			case "haxe.io.Bytes":
				// Where the library writes how many bytes it returned.
				args.push({name: "out", type: Native.type("hl.Ref<Int>")});
				var counted = Native.call("", name, passed.concat([macro outLength]));
				// Native code may give empty bytes as a null pointer.
				var none = Native.nullable(ret) ? macro null : macro haxe.io.Bytes.alloc(0);
				macro {
					var outLength = 0;
					var data = $counted;
					return data == null ? $none : @:privateAccess new haxe.io.Bytes(data, outLength);
				};
			case "hxn.Handle": macro return $made;
			// The library owns the cell; the returned atomic keeps its owner.
			case "hxn.AtomicInt":
				cells = true;
				macro {
					var owner = $made;
					return hxn.AtomicInt.over(hxnAtomicCell(owner), owner);
				};
			case other: Native.unsupported(declaration, "HashLink", 'result of type $other');
		}
	}

	// A lent function as HashLink native code calls it: text and an object's
	// pointer arrive as VM bytes, and bytes with their length, since native
	// code cannot make a `String` or a `haxe.io.Bytes`. The wrapper converts
	// them and calls the binding's guard.
	function lent(declaration:Field, given:Map<String, String>, name:String, type:ComplexType):{type:ComplexType, wrapper:Expr} {
		var shape = Native.lentTypes(type);
		var nativeArgs:Array<ComplexType> = [];
		var wrapperArgs:Array<FunctionArg> = [];
		var handed:Array<Expr> = [];
		for (position in 0...shape.args.length) {
			var a = 'a$position';
			switch Native.kind(shape.args[position]) {
				case "String":
					nativeArgs.push(Native.type("hl.Bytes"));
					wrapperArgs.push({name: a, type: Native.type("hl.Bytes")});
					handed.push(macro @:privateAccess String.fromUCS2($i{a}));
				case "haxe.io.Bytes":
					nativeArgs.push(Native.type("hl.Bytes"));
					nativeArgs.push(Native.type("Int"));
					wrapperArgs.push({name: a, type: Native.type("hl.Bytes")});
					wrapperArgs.push({name: a + "Length", type: Native.type("Int")});
					handed.push(macro @:privateAccess new haxe.io.Bytes($i{a}, $i{a + "Length"}));
				case "hxn.Handle":
					var export = given['$name.$position'];
					if (export == null)
						Context.error('hxn.Native: ${declaration.name} lends a function whose argument $position is an object, with no @:hxn.given for it',
							declaration.pos);
					var adopt = 'hxnAdopt_$export';
					if (!Lambda.exists(extra, field -> field.name == adopt))
						extra.push(primitive(adopt, library + "_" + export, [{name: "raw", type: Native.type("hl.Bytes")}],
							Native.type("hxn.Handle")));
					nativeArgs.push(Native.type("hl.Bytes"));
					wrapperArgs.push({name: a, type: Native.type("hl.Bytes")});
					handed.push(macro $i{adopt}($i{a}));
				case "Bool" | "Int" | "Float":
					nativeArgs.push(shape.args[position]);
					wrapperArgs.push({name: a, type: shape.args[position]});
					handed.push(macro $i{a});
				case other:
					Native.unsupported(declaration, "HashLink", 'lent function taking $other');
			}
		}
		var body = Native.call("", name, handed);
		var wrapper = Native.closure(wrapperArgs, shape.ret, macro return $body);
		return {type: TFunction(nativeArgs, shape.ret), wrapper: wrapper};
	}
}
#end
