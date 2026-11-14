package hxn.native;

#if macro
import haxe.macro.Context;
import haxe.macro.Expr;
import hxn.Native;

// The hxcpp implementation: an extern class over the library's glue, and the
// declaration as a function that converts around it. A returned handle is
// given the finalizer that releases it.
@:noCompletion class Hxcpp {
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
			case "hxn.AtomicInt" | "hxn.AtomicBool":
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
			// Null when the library returned none, as `thrown_take` does.
			case "haxe.io.Bytes":
				macro {
					var data = $made;
					return data == null ? null : haxe.io.Bytes.ofData(data);
				};
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
