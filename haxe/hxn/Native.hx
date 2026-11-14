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
	`haxe.io.Bytes`, `hxn.Handle`, `hxn.AtomicInt` or `hxn.AtomicBool`, `Void`
	for a result, and
	for a lent function a function type of those: the binding's guard, which
	reports a failure by returning `false` or `null`.

	Each runtime's implementation is a class in `hxn.native`: `HashLink` and
	`Hxcpp`.

	`@:hxn.given("name.position", export)` names, for a lent function's
	argument that is an object, the library's export that gives HashLink
	native code's pointer to it a type: `counter_adopt_lent`.
**/
class Native {
	#if macro
	public static function library(name:String):Array<Field> {
		var fields = Context.getBuildFields();
		var declared = [for (field in fields) if (isDeclaration(field)) field];
		var kept = [for (field in fields) if (!isDeclaration(field)) field];
		if (Context.defined("hl"))
			return kept.concat(new hxn.native.HashLink(name).implement(declared));
		if (Context.defined("cpp"))
			return kept.concat(new hxn.native.Hxcpp(name).implement(declared));
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

	// For each lent function's argument that is an object, by
	// `name.position`, the export that types HashLink's pointer to it.
	public static function given(field:Field):Map<String, String> {
		var out = new Map<String, String>();
		for (entry in field.meta)
			if (entry.name == ":hxn.given" && entry.params.length == 2)
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
