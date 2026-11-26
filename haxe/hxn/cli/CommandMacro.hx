package hxn.cli;

#if macro
import haxe.macro.Context;
import haxe.macro.Expr;
import haxe.macro.Type;
import hxn.cli.Spec.Kind;

using StringTools;

/**
	`CommandMacro` builds each class that implements `Command`. It reads the
	fields marked with `@:arg` and `@:option`, checks them, and adds `spec`,
	which returns what it read.
**/
class CommandMacro {
	public static function build():Array<Field> {
		var fields = Context.getBuildFields();
		var local = Context.getLocalClass().get();
		var arguments:Array<Expr> = [];
		var options:Array<Expr> = [];
		var longs = ["--help"];
		var shorts = ["-h"];
		var last:Null<Kind> = null;
		for (field in fields) {
			var argument = find(field, ":arg");
			var option = find(field, ":option");
			if (argument == null && option == null)
				continue;
			var type:Null<ComplexType>;
			var init:Null<Expr>;
			switch field.kind {
				case FVar(t, e) | FProp(_, _, t, e):
					type = t;
					init = e;
				case _:
					Context.error('${field.name}: @:arg and @:option mark variables', field.pos);
			}
			// A constant initializer gives the type of a field that has none.
			if (type == null && init != null)
				type = switch init.expr {
					case EConst(CString(_)): macro :String;
					case EConst(CInt(_)): macro :Int;
					case EConst(CFloat(_)): macro :Float;
					case EConst(CIdent("true" | "false")): macro :Bool;
					case _: null;
				}
			if (type == null)
				Context.error('${field.name}: give the type of an argument or option', field.pos);
			if (field.access.contains(AStatic))
				Context.error('${field.name}: arguments and options are instance variables', field.pos);
			var nullable = false;
			var kind = kindOf(type, field, ref -> nullable = ref);
			var doc = field.doc == null ? "" : clean(field.doc);
			var value = text(find(field, ":value"), 0);
			var fallback = init == null ? null : constant(init);
			if (argument != null) {
				if (last == Rest || last == Raw)
					Context.error('${field.name}: the argument that takes the rest of the arguments is the last one', field.pos);
				if (kind == Flag)
					Context.error('${field.name}: an argument cannot be a Bool, which an option can', field.pos);
				if (kind == List)
					kind = find(field, ":raw") != null ? Raw : Rest;
				last = kind;
				var choices = [for (e in params(find(field, ":choices"))) stringOf(e)];
				var required = init == null && !nullable && kind != Rest && kind != Raw;
				arguments.push(macro {
					field: $v{field.name},
					value: $v{value == null ? field.name : value},
					doc: $v{doc},
					kind: (cast $v{(kind : String)} : hxn.cli.Spec.Kind),
					required: $v{required},
					fallback: $v{fallback},
					choices: $v{choices}
				});
			} else {
				var names = params(option);
				var long = names.length > 0 ? stringOf(names[0]) : "--" + dashed(field.name);
				var short = names.length > 1 ? stringOf(names[1]) : null;
				if (!~/^--[a-z0-9][a-z0-9-]*$/.match(long))
					Context.error('${field.name}: $long is not an option name such as --name', field.pos);
				if (short != null && !~/^-[A-Za-z0-9]$/.match(short))
					Context.error('${field.name}: $short is not a short option such as -n', field.pos);
				if (longs.contains(long) || (short != null && shorts.contains(short)))
					Context.error('${field.name}: another option of the command is called ${longs.contains(long) ? long : short}', field.pos);
				longs.push(long);
				if (short != null)
					shorts.push(short);
				options.push(macro {
					field: $v{field.name},
					long: $v{long},
					short: $v{short},
					value: $v{value == null ? dashed(field.name) : value},
					doc: $v{doc},
					kind: (cast $v{(kind : String)} : hxn.cli.Spec.Kind),
					env: $v{text(find(field, ":env"), 0)},
					fallback: $v{fallback}
				});
			}
		}
		var name = text(Lambda.find(local.meta.get(), m -> m.name == ":command"), 0);
		if (name == null)
			name = dashed(local.name.endsWith("Command") && local.name != "Command" ? local.name.substr(0, local.name.length - 7) : local.name);
		var doc = local.doc == null ? "" : clean(local.doc);
		var pos = Context.currentPos();
		fields.push({
			name: "spec",
			access: [APublic],
			kind: FFun({
				args: [],
				ret: macro :hxn.cli.Spec,
				expr: macro return {
					name: $v{name},
					doc: $v{doc},
					arguments: [$a{arguments}],
					options: [$a{options}]
				}
			}),
			pos: pos
		});
		if (!Lambda.exists(fields, f -> f.name == "new"))
			fields.push({
				name: "new",
				access: [APublic],
				kind: FFun({args: [], expr: macro {}}),
				pos: pos
			});
		return fields;
	}

	// Returns the kind of value that `type` takes, and passes whether it is `Null`.
	static function kindOf(type:ComplexType, field:Field, nullable:Bool->Void):Kind {
		var resolved = Context.resolveType(type, field.pos);
		switch resolved {
			case TAbstract(_.get() => {pack: [], name: "Null"}, [inner]):
				nullable(true);
				resolved = inner;
			case TType(_.get() => {pack: [], name: "Null"}, [inner]):
				nullable(true);
				resolved = inner;
			case _:
		}
		return switch Context.followWithAbstracts(resolved) {
			case TInst(_.get() => {pack: [], name: "String"}, _): Text;
			case TAbstract(_.get() => {pack: [], name: "Int"}, _): Integer;
			case TAbstract(_.get() => {pack: [], name: "Float"}, _): Number;
			case TAbstract(_.get() => {pack: [], name: "Bool"}, _): Flag;
			case TInst(_.get() => {pack: [], name: "Array"}, [Context.follow(_) => TInst(_.get() => {pack: [], name: "String"}, _)]): List;
			case _: Context.error('${field.name}: an argument or option is a String, Int, Float, Bool or Array<String>', field.pos);
		}
	}

	static function find(field:Field, name:String):Null<MetadataEntry> {
		if (field.meta == null)
			return null;
		return Lambda.find(field.meta, m -> m.name == name);
	}

	static function params(entry:Null<MetadataEntry>):Array<Expr>
		return entry == null || entry.params == null ? [] : entry.params;

	static function text(entry:Null<MetadataEntry>, index:Int):Null<String> {
		var all = params(entry);
		return all.length > index ? stringOf(all[index]) : null;
	}

	static function stringOf(e:Expr):String {
		return switch e.expr {
			case EConst(CString(s, _)): s;
			case _: Context.error("expected a string", e.pos);
		}
	}

	// Returns the text of a constant initializer, for the help, or null.
	static function constant(e:Expr):Null<String> {
		return switch e.expr {
			case EConst(CString(s, _)): s;
			case EConst(CInt(s)) | EConst(CFloat(s)): s;
			case EConst(CIdent("true")): "true";
			case _: null;
		}
	}

	// Returns `name` in lower case with a hyphen before each word, as in `--content-addressed`.
	static function dashed(name:String):String {
		var out = new StringBuf();
		for (i in 0...name.length) {
			var c = name.charAt(i);
			if (c != c.toLowerCase() && i > 0)
				out.add("-");
			out.add(c == "_" ? "-" : c.toLowerCase());
		}
		return out.toString();
	}

	// Returns a doc comment with the lines of each paragraph joined, and a blank line between paragraphs.
	static function clean(doc:String):String {
		var paragraphs:Array<String> = [];
		var words:Array<String> = [];
		for (raw in doc.split("\n")) {
			var line = raw.trim();
			if (line.startsWith("*"))
				line = line.substr(1).trim();
			if (line != "") {
				words.push(line);
				continue;
			}
			if (words.length > 0)
				paragraphs.push(words.join(" "));
			words = [];
		}
		if (words.length > 0)
			paragraphs.push(words.join(" "));
		return paragraphs.join("\n\n");
	}
}
#end
