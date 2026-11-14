package hxn;

#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

using haxe.macro.ComplexTypeTools;
using haxe.macro.ExprTools;
#end

/**
	Implements a library's native class for the runtime being compiled.

	hxn-gen writes the class as declarations in Haxe types, the same on every
	runtime: `static function greeter_greet(self:hxn.Handle, name:String):String;`.
	`@:build(hxn.Native.library("greeter"))` gives each a body. On HashLink it
	calls a primitive the library exports, `greeter_greeter_greet`, converting
	text from UCS-2, bytes to a pointer and a length, and each handle to the
	abstract type the primitive declares. On hxcpp it calls the function the
	library's glue header declares, `greeter_glue::greeter_greet`, and gives a
	returned handle a finalizer that releases it.

	A declaration's types are `Bool`, `Int`, `Float`, `haxe.Int64`, `String`,
	`haxe.io.Bytes`, `hxn.Handle`, `hxn.AtomicInt`, `Void` for a result, and
	for a lent function a function type of those: the binding's guard, which
	reports a failure by returning `false` or `null`.

	`@:hxn.abstract(name, type)` names the HashLink abstract type of a handle:
	of the argument `name`, of the result for `return`, or of a lent
	function's argument as `name.position`. A returned `hxn.AtomicInt` names
	its cell's abstract type the same way.
**/
class Native {
	#if macro
	public static function library(name:String):Array<Field> {
		var fields = Context.getBuildFields();
		var declared = [for (field in fields) if (isDeclaration(field)) field];
		var kept = [for (field in fields) if (!isDeclaration(field)) field];
		if (Context.defined("hl"))
			return kept.concat(new HashLink(name).implement(declared));
		if (Context.defined("cpp"))
			return kept.concat(new Hxcpp(name).implement(declared));
		// Elsewhere, such as an editor's completion, the declarations are only
		// typed.
		return kept.concat([for (field in declared) stub(field)]);
	}

	static function isDeclaration(field:Field):Bool {
		return switch field.kind {
			case FFun(f): f.expr == null;
			case _: false;
		}
	}

	static function stub(declared:Field):Field {
		var f = functionOf(declared);
		var body = kind(f.ret) == "Void" ? macro {} : macro return cast null;
		return method(declared.name, f.args, returnType(f), body, [], [APublic, AStatic]);
	}

	public static function functionOf(field:Field):Function {
		return switch field.kind {
			case FFun(f): f;
			case _: throw "not a function";
		}
	}

	public static function returnType(f:Function):ComplexType
		return f.ret == null ? type("Void") : f.ret;

	// The abstract type each handle crosses as on HashLink, by argument name,
	// `return`, or `name.position` for a lent function's argument.
	public static function abstracts(field:Field):Map<String, String> {
		var out = new Map<String, String>();
		for (entry in field.meta)
			if (entry.name == ":hxn.abstract" && entry.params.length == 2)
				out[entry.params[0].getValue()] = entry.params[1].getValue();
		return out;
	}

	public static function kind(type:Null<ComplexType>):String {
		return switch type {
			case null: "Void";
			case TFunction(_, _): "function";
			case TParent(inner): kind(inner);
			case other: other.toString();
		}
	}

	public static function type(text:String):ComplexType {
		return switch Context.parse('(null : $text)', Context.currentPos()).expr {
			case EParenthesis({expr: ECheckType(_, parsed)}) | ECheckType(_, parsed): parsed;
			case _: throw 'not a type: $text';
		}
	}

	public static function call(target:String, name:String, args:Array<Expr>):Expr {
		if (target == "")
			return macro $i{name}($a{args});
		var path = target.split(".").concat([name]);
		return macro $p{path}($a{args});
	}

	public static function method(name:String, args:Array<FunctionArg>, ret:ComplexType, body:Expr, meta:Metadata,
			access:Array<Access>):Field {
		return {
			name: name,
			access: access,
			kind: FFun({args: args, ret: ret, expr: body}),
			meta: meta,
			pos: Context.currentPos(),
		};
	}

	public static function closure(args:Array<FunctionArg>, ret:ComplexType, body:Expr):Expr {
		return {expr: EFunction(FAnonymous, {args: args, ret: ret, expr: body}), pos: Context.currentPos()};
	}

	public static function nothing(type:String):Expr {
		return switch type {
			case "Void": macro {};
			case "Bool": macro return false;
			case "Int": macro return 0;
			case "Float": macro return 0.;
			case _: macro return cast null;
		}
	}

	// A lent function's argument and result types.
	public static function lentTypes(type:ComplexType):{args:Array<ComplexType>, ret:ComplexType} {
		return switch type {
			case TFunction(args, ret): {args: [for (arg in args) unnamed(arg)], ret: ret};
			case TParent(inner): lentTypes(inner);
			case _: throw "not a function type";
		}
	}

	static function unnamed(type:ComplexType):ComplexType {
		return switch type {
			case TNamed(_, inner): inner;
			case other: other;
		}
	}

	public static function unsupported(declaration:Field, runtime:String, what:String):Dynamic {
		Context.error('hxn.Native: ${declaration.name} has a $what, which $runtime has no form for', declaration.pos);
		return null;
	}
	#end
}

#if macro
// The HashLink implementation: a primitive for each declaration, and the
// declaration as a function that converts its arguments and result around it.
private class HashLink {
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

// The hxcpp implementation: an extern class over the library's glue, and the
// declaration as a function that converts around it. A returned handle is
// given the finalizer that releases it.
private class Hxcpp {
	final library:String;
	final externs:Array<Field> = [];
	final glue:String;
	final glueName:String;
	final local:haxe.macro.Type.ClassType;
	var cells = false;
	static final RAW = "cpp.RawPointer<cpp.Void>";

	public function new(library:String) {
		this.library = library;
		local = Context.getLocalClass().get();
		glueName = local.name + "Glue";
		glue = local.pack.concat([glueName]).join(".");
	}

	function external(name:String, args:Array<FunctionArg>, ret:ComplexType) {
		if (Lambda.exists(externs, field -> field.name == name))
			return;
		externs.push({
			name: name,
			access: [APublic, AStatic],
			kind: FFun({args: args, ret: ret, expr: null}),
			meta: [{name: ":native", params: [macro $v{library + "_glue::" + name}], pos: Context.currentPos()}],
			pos: Context.currentPos(),
		});
	}

	public function implement(declared:Array<Field>):Array<Field> {
		var out:Array<Field> = [];
		external("release", [{name: "self", type: Native.type(RAW)}], Native.type("Void"));
		out.push(Native.method("hxnFinalize", [{name: "self", type: Native.type("hxn.Handle")}], Native.type("Void"),
			Native.call(glue, "release", [macro self.raw]), [], [AStatic]));
		// A value the library returned, released by the collector.
		out.push(Native.method("hxnOwn", [{name: "raw", type: Native.type(RAW)}], Native.type("hxn.Handle"), macro {
			if (raw == null)
				return null;
			var handle = new hxn.Handle(raw);
			cpp.vm.Gc.setFinalizer(handle, cpp.Function.fromStaticFunction(hxnFinalize));
			return handle;
		}, [], [APublic, AStatic]));
		for (declaration in declared) {
			var f = Native.functionOf(declaration);
			var args:Array<FunctionArg> = [];
			var passed:Array<Expr> = [];
			for (arg in f.args)
				argument(declaration, arg, args, passed);
			var made = Native.call(glue, declaration.name, passed);
			var ret = Native.returnType(f);
			external(declaration.name, args, externalResult(declaration, ret));
			out.push(Native.method(declaration.name, f.args, ret, result(declaration, ret, made), [], [APublic, AStatic]));
		}
		if (cells)
			external("atomic_cell", [{name: "self", type: Native.type(RAW)}], Native.type("cpp.RawPointer<Int>"));
		Context.defineType({
			pack: local.pack,
			name: glueName,
			pos: Context.currentPos(),
			kind: TDClass(null, [], false, false, false),
			isExtern: true,
			meta: [
				{name: ":include", params: [macro $v{library + "_glue.h"}], pos: Context.currentPos()},
				{name: ":noCompletion", params: [], pos: Context.currentPos()}
			],
			fields: externs,
		});
		return out;
	}

	function argument(declaration:Field, arg:FunctionArg, args:Array<FunctionArg>, passed:Array<Expr>) {
		var name = arg.name;
		switch Native.kind(arg.type) {
			case "String" | "Bool" | "Int" | "Float":
				args.push({name: name, type: arg.type});
				passed.push(macro $i{name});
			case "haxe.Int64":
				args.push({name: name, type: Native.type("cpp.Int64")});
				passed.push(macro $i{name});
			case "haxe.io.Bytes":
				args.push({name: name, type: Native.type("haxe.io.BytesData")});
				passed.push(macro $i{name}.getData());
			// Null is passed through; hxcpp does not check an object for null
			// before reading its field.
			case "hxn.Handle":
				args.push({name: name, type: Native.type(RAW)});
				passed.push(macro $i{name} == null ? null : $i{name}.raw);
			case "hxn.AtomicInt":
				args.push({name: name, type: Native.type("cpp.RawPointer<Int>")});
				passed.push(macro $i{name} == null ? null : $i{name}.pointer.raw);
			case "function":
				args.push({name: name, type: Native.type("Dynamic")});
				passed.push(lent(declaration, name, arg.type));
			case other:
				Native.unsupported(declaration, "hxcpp", 'parameter of type $other');
		}
	}

	function externalResult(declaration:Field, ret:ComplexType):ComplexType {
		return switch Native.kind(ret) {
			case "Void" | "String" | "Bool" | "Int" | "Float": ret;
			case "haxe.Int64": Native.type("cpp.Int64");
			case "haxe.io.Bytes": Native.type("haxe.io.BytesData");
			case "hxn.Handle" | "hxn.AtomicInt": Native.type(RAW);
			case other: Native.unsupported(declaration, "hxcpp", 'result of type $other');
		}
	}

	function result(declaration:Field, ret:ComplexType, made:Expr):Expr {
		return switch Native.kind(ret) {
			case "Void": made;
			case "String" | "Bool" | "Int" | "Float" | "haxe.Int64": macro return $made;
			case "haxe.io.Bytes": macro return haxe.io.Bytes.ofData($made);
			case "hxn.Handle": macro return hxnOwn($made);
			// The library owns the cell; the returned atomic keeps its owner.
			case "hxn.AtomicInt":
				cells = true;
				var cell = Native.call(glue, "atomic_cell", [macro owner.raw]);
				macro {
					var owner = hxnOwn($made);
					return hxn.AtomicInt.over(cpp.Pointer.fromRaw($cell), owner);
				};
			case other: Native.unsupported(declaration, "hxcpp", 'result of type $other');
		}
	}

	// A lent function as the glue calls it: bytes arrive as their data, and an
	// object as its pointer, which is given an owner before anything else.
	function lent(declaration:Field, name:String, type:ComplexType):Expr {
		var shape = Native.lentTypes(type);
		var wrapperArgs:Array<FunctionArg> = [];
		var given:Array<Expr> = [];
		for (position in 0...shape.args.length) {
			var a = 'a$position';
			switch Native.kind(shape.args[position]) {
				case "haxe.io.Bytes":
					wrapperArgs.push({name: a, type: Native.type("haxe.io.BytesData")});
					given.push(macro haxe.io.Bytes.ofData($i{a}));
				case "hxn.Handle":
					wrapperArgs.push({name: a, type: Native.type("cpp.Pointer<cpp.Void>")});
					given.push(macro hxnOwn($i{a}.raw));
				case "String" | "Bool" | "Int" | "Float":
					wrapperArgs.push({name: a, type: shape.args[position]});
					given.push(macro $i{a});
				case other:
					Native.unsupported(declaration, "hxcpp", 'lent function taking $other');
			}
		}
		var body = Native.call("", name, given);
		return switch Native.kind(shape.ret) {
			case "haxe.io.Bytes":
				Native.closure(wrapperArgs, Native.type("haxe.io.BytesData"), macro {
					var result = $body;
					return result == null ? null : result.getData();
				});
			case _:
				Native.closure(wrapperArgs, shape.ret, macro return $body);
		}
	}
}
#end
