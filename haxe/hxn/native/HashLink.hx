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
	var cell:Null<String>;

	public function new(library:String)
		this.library = library;

	function primitive(name:String, full:String, args:Array<FunctionArg>, ret:ComplexType):Field {
		return Native.method(name, args, ret, Native.nothing(Native.kind(ret)), [
			{name: ":hlNative", params: [macro $v{library}, macro $v{full}], pos: Context.currentPos()}
		], [AStatic]);
	}

	function handle(abstracts:Map<String, String>, key:String):ComplexType {
		return Native.type(abstracts.exists(key) ? 'hl.Abstract<"${abstracts[key]}">' : "hxn.Handle");
	}

	public function implement(declared:Array<Field>):Array<Field> {
		var out = [];
		for (declaration in declared) {
			var f = Native.functionOf(declaration);
			var abstracts = Native.abstracts(declaration);
			var args:Array<FunctionArg> = [];
			var passed:Array<Expr> = [];
			for (arg in f.args)
				argument(declaration, abstracts, arg, args, passed);
			var name = declaration.name + "Primitive";
			var body = result(declaration, abstracts, Native.returnType(f), name, args, passed);
			out.push(primitive(name, library + "_" + declaration.name, args, primitiveResult(declaration, abstracts, f)));
			out.push(Native.method(declaration.name, f.args, Native.returnType(f), body, [], [APublic, AStatic]));
		}
		if (cell != null)
			extra.push(primitive("hxnAtomicCell", library + "_atomic_cell", [{name: "self", type: Native.type('hl.Abstract<"$cell">')}],
				Native.type("hl.Ref<Int>")));
		return out.concat(extra);
	}

	function argument(declaration:Field, abstracts:Map<String, String>, arg:FunctionArg, args:Array<FunctionArg>,
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
				args.push({name: name, type: handle(abstracts, name)});
				passed.push(macro cast $i{name});
			case "hxn.AtomicInt":
				args.push({name: name, type: Native.type("hl.Ref<Int>")});
				passed.push(macro $i{name} == null ? null : $i{name}.pointer);
			case "function":
				var lent = lent(declaration, abstracts, name, arg.type);
				args.push({name: name, type: lent.type});
				passed.push(lent.wrapper);
			case other:
				Native.unsupported(declaration, "HashLink", 'parameter of type $other');
		}
	}

	function primitiveResult(declaration:Field, abstracts:Map<String, String>, f:Function):ComplexType {
		return switch Native.kind(f.ret) {
			case "Void" | "Bool" | "Int" | "Float": Native.returnType(f);
			case "haxe.Int64": Native.type("hl.I64");
			case "String" | "haxe.io.Bytes": Native.type("hl.Bytes");
			case "hxn.Handle": handle(abstracts, "return");
			case "hxn.AtomicInt": handle(abstracts, "return");
			case other: Native.unsupported(declaration, "HashLink", 'result of type $other');
		}
	}

	function result(declaration:Field, abstracts:Map<String, String>, ret:ComplexType, name:String, args:Array<FunctionArg>,
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
				macro {
					var outLength = 0;
					var data = $counted;
					return @:privateAccess new haxe.io.Bytes(data, outLength);
				};
			case "hxn.Handle": macro return cast $made;
			// The library owns the cell; the returned atomic keeps its owner.
			case "hxn.AtomicInt":
				cell = abstracts["return"];
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
	function lent(declaration:Field, abstracts:Map<String, String>, name:String, type:ComplexType):{type:ComplexType, wrapper:Expr} {
		var shape = Native.lentTypes(type);
		var nativeArgs:Array<ComplexType> = [];
		var wrapperArgs:Array<FunctionArg> = [];
		var given:Array<Expr> = [];
		for (position in 0...shape.args.length) {
			var a = 'a$position';
			switch Native.kind(shape.args[position]) {
				case "String":
					nativeArgs.push(Native.type("hl.Bytes"));
					wrapperArgs.push({name: a, type: Native.type("hl.Bytes")});
					given.push(macro @:privateAccess String.fromUCS2($i{a}));
				case "haxe.io.Bytes":
					nativeArgs.push(Native.type("hl.Bytes"));
					nativeArgs.push(Native.type("Int"));
					wrapperArgs.push({name: a, type: Native.type("hl.Bytes")});
					wrapperArgs.push({name: a + "Length", type: Native.type("Int")});
					given.push(macro @:privateAccess new haxe.io.Bytes($i{a}, $i{a + "Length"}));
				case "hxn.Handle":
					var of = abstracts['$name.$position'];
					if (of == null)
						Context.error('hxn.Native: ${declaration.name} lends a function whose argument $position is an object of no named type',
							declaration.pos);
					var adopt = 'hxnAdoptLent_$of';
					if (!Lambda.exists(extra, field -> field.name == adopt))
						extra.push(primitive(adopt, of + "_adopt_lent", [{name: "raw", type: Native.type("hl.Bytes")}],
							Native.type('hl.Abstract<"$of">')));
					nativeArgs.push(Native.type("hl.Bytes"));
					wrapperArgs.push({name: a, type: Native.type("hl.Bytes")});
					given.push(macro cast $i{adopt}($i{a}));
				case "Bool" | "Int" | "Float":
					nativeArgs.push(shape.args[position]);
					wrapperArgs.push({name: a, type: shape.args[position]});
					given.push(macro $i{a});
				case other:
					Native.unsupported(declaration, "HashLink", 'lent function taking $other');
			}
		}
		var body = Native.call("", name, given);
		var wrapper = Native.closure(wrapperArgs, shape.ret, macro return $body);
		return {type: TFunction(nativeArgs, shape.ret), wrapper: wrapper};
	}
}
#end
