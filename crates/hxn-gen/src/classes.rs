//! A library's Haxe classes, one set for every runtime.
//!
//! The classes call the library through one native class, written as
//! declarations in Haxe types: `hxn.Native` in the hxn haxelib implements it
//! for the runtime being compiled, over HashLink primitives or hxcpp's glue.
//! A handle is an `hxn.Handle`, which owns the value on either runtime, so the
//! classes themselves have nothing particular to one runtime.

use crate::data;
use crate::haxe::{
    File, HEADER, MEMORY_DOCS, NAMESPACE_DOCS, SAME_DOCS, THROWN_DOCS, UPGRADE_DOCS, WEAK_DOCS,
    body, build_xml, close, docs, grown, in_package, meter, read, signature, thrown, thrown_read,
    written,
};
use crate::later::{self, Kind};
use crate::lent;
use crate::model::{Data, Interface, Library, Method, Type};
use crate::names::{
    has_receiver, haxe_param, local, memory_class, native_class, thrown_class, weak_class,
};

/// The library's Haxe classes: the same for HashLink, hxcpp and their wasm
/// forms.
pub fn haxe_classes(library: &Library) -> Vec<File> {
    let mut files: Vec<File> = library
        .interfaces
        .iter()
        .map(|interface| File {
            path: format!("{}.hx", interface.name),
            text: module(library, interface),
        })
        .collect();
    for interface in library.interfaces.iter().filter(|i| i.shared) {
        files.push(File {
            path: format!("{}.hx", weak_class(&interface.name)),
            text: weak(library, interface),
        });
    }
    files.extend(data::haxe_files(library));
    if !library.errors().is_empty() {
        files.push(File {
            path: format!("{}.hx", thrown_class(library)),
            text: thrown_file(library),
        });
    }
    files.extend(later_files(library));
    files.push(File {
        path: format!("{}.hx", memory_class(library)),
        text: memory(library),
    });
    files.push(File {
        path: format!("{}.hx", native_class(library)),
        text: natives(library),
    });
    in_package(library, files)
}

// The Haxe type a value crosses the native class as.
fn native_type(ty: &Type) -> String {
    match ty {
        Type::Void => String::from("Void"),
        Type::Bool => String::from("Bool"),
        Type::I32 => String::from("Int"),
        Type::I64 => String::from("haxe.Int64"),
        Type::F64 => String::from("Float"),
        Type::Str => String::from("String"),
        Type::Bytes | Type::BytesMut => String::from("haxe.io.Bytes"),
        Type::AtomicInt => String::from("hxn.AtomicInt"),
        Type::AtomicBool => String::from("hxn.AtomicBool"),
        Type::Lent { params, returns } => guard_type(params, returns),
        Type::Handle { .. }
        | Type::Future(_)
        | Type::Stream(_)
        | Type::Signal(_)
        | Type::List { .. } => String::from("hxn.Handle"),
        Type::Data(_) | Type::Fallible { .. } => unreachable!("lowered before it crosses"),
    }
}

// A lent function as the native class takes it: the binding's guard, over
// the types its arguments and result cross as. It reports a failure by
// returning false, or null.
fn guard_type(params: &[Type], returns: &Type) -> String {
    let taken: Vec<String> = params
        .iter()
        .map(|ty| native_type(&lent::crossed(ty)))
        .collect();
    let returned = match lent::crossed(returns) {
        Type::Void => String::from("Bool"),
        Type::Bytes => String::from("haxe.io.Bytes"),
        other => format!("Null<{}>", native_type(&other)),
    };
    format!("({}) -> {returned}", taken.join(", "))
}

// One declaration of the native class. A lent function's argument that is
// an object names, as `name.position`, the export that gives HashLink
// native code's pointer to it a type.
fn declaration(
    name: &str,
    params: &[(String, Type)],
    returns: &Type,
    given: &[(String, String)],
) -> String {
    let mut out = String::new();
    for (key, export) in given {
        out.push_str(&format!("\t@:hxn.given(\"{key}\", \"{export}\")\n"));
    }
    let params: Vec<String> = params
        .iter()
        .map(|(name, ty)| format!("{name}:{}", native_type(ty)))
        .collect();
    // A returned flag is its cell, which the method makes the flag over.
    let returned = match returns {
        Type::AtomicBool => String::from("hxn.AtomicInt"),
        other => native_type(other),
    };
    out.push_str(&format!(
        "\tstatic function {name}({}):{returned};\n",
        params.join(", "),
    ));
    out
}

fn method_declaration(interface: &Interface, method: &Method) -> String {
    let mut params = Vec::new();
    let mut given = Vec::new();
    if has_receiver(method) {
        params.push((String::from("self"), handle(&interface.name)));
    }
    for param in &method.params {
        let name = haxe_param(&param.name);
        if let Type::Lent { params: taken, .. } = &param.ty {
            for (position, ty) in taken.iter().enumerate() {
                if let Type::Handle { interface, .. } = ty {
                    given.push((format!("{name}.{position}"), local(interface, "adopt_lent")));
                }
            }
        }
        params.push((name, param.ty.clone()));
    }
    declaration(
        &local(&interface.name, &method.name),
        &params,
        &method.returns,
        &given,
    )
}

fn handle(interface: &str) -> Type {
    Type::Handle {
        interface: interface.to_owned(),
        nullable: false,
    }
}

// The library's native class, as declarations the hxn haxelib's macro
// implements. hxcpp reads the build XML and the include; HashLink does not.
fn natives(library: &Library) -> String {
    let this = |name: &str| vec![(String::from("self"), handle(name))];
    let mut out = format!(
        "{HEADER}/** The library's functions; `hxn.Native` implements them for the runtime compiled. **/\n@:build(hxn.Native.library(\"{}\"))\n{}@:noCompletion class {} {{\n",
        library.name,
        build_xml(library),
        native_class(library)
    );
    out.push_str(&declaration("memory_held", &[], &Type::F64, &[]));
    out.push_str(&declaration("memory_wanted", &[], &Type::Bool, &[]));
    for interface in &library.interfaces {
        let name = &interface.name;
        for method in &interface.methods {
            out.push_str(&method_declaration(interface, method));
        }
        if interface.namespace {
            continue;
        }
        out.push_str(&declaration(
            &local(name, "drop"),
            &this(name),
            &Type::Void,
            &[],
        ));
        if interface.shared {
            let mut both = this(name);
            both.push((String::from("other"), handle(name)));
            out.push_str(&declaration(&local(name, "same"), &both, &Type::Bool, &[]));
            out.push_str(&declaration(
                &local(name, "weak"),
                &this(name),
                &handle(name),
                &[],
            ));
            out.push_str(&declaration(
                &local(name, "weak_upgrade"),
                &this(name),
                &handle(name),
                &[],
            ));
            out.push_str(&declaration(
                &local(name, "weak_close"),
                &this(name),
                &Type::Void,
                &[],
            ));
        }
    }
    let used = later::used(library);
    if !used.is_empty() {
        out.push_str(&declaration(
            "outbox_wait",
            &[(String::from("milliseconds"), Type::I32)],
            &Type::Bool,
            &[],
        ));
        out.push_str(&declaration("outbox_next", &[], &Type::I32, &[]));
    }
    for (kind, carried) in &used {
        for (call, params, _, _) in later::calls(*kind) {
            let mut taken = this(kind.name());
            for name in later::names(params) {
                taken.push((name.to_owned(), Type::I32));
            }
            out.push_str(&declaration(
                &format!("{}_{call}", kind.name()),
                &taken,
                &Type::I32,
                &[],
            ));
        }
        for carried in carried {
            // A future or stream of nothing takes whether it was delivered.
            let returned = match later::crossed(carried) {
                Type::Void => Type::Bool,
                other => other,
            };
            out.push_str(&declaration(
                &format!("{}_{}_{}", kind.name(), kind.take(), later::suffix(carried)),
                &this(kind.name()),
                &returned,
                &[],
            ));
        }
    }
    if !library.errors().is_empty() {
        out.push_str(&declaration("thrown_take", &[], &Type::Bytes, &[]));
    }
    out.push_str("}\n");
    out
}

// A lent function's guard: it gives each object it is passed an owner, reads
// the data it is passed, calls the function, and returns its result as it
// crosses, or reports a throw with `hxn.Callback.failed` and returns false
// or null.
fn guard(name: &str, params: &[Type], returns: &Type) -> String {
    let declared: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| format!("a{n}:{}", native_type(&lent::crossed(ty))))
        .collect();
    let owned: String = params
        .iter()
        .enumerate()
        .filter_map(|(n, ty)| match ty {
            Type::Handle { interface, .. } => {
                Some(format!("\n\t\t\tvar o{n} = {interface}.hxnAdopt(a{n});"))
            }
            _ => None,
        })
        .collect();
    let mut read = String::new();
    let passed: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match ty {
            Type::Handle { .. } => format!("o{n}"),
            Type::Data(data) if lent::crossed(ty) == Type::Bytes => {
                read.push_str(&format!(
                    "var d{n}:{} = {{\n\t\t\t\t\tvar input = new haxe.io.BytesInput(a{n});\n\t\t\t\t\t{};\n\t\t\t\t}};\n\t\t\t\t",
                    data::haxe_type(data),
                    data::haxe_read(data)
                ));
                format!("d{n}")
            }
            _ => format!("a{n}"),
        })
        .collect();
    let call = format!("{name}({})", passed.join(", "));
    let refused = "\n\t\t\t\tif (result == null)\n\t\t\t\t\tthrow new haxe.Exception(\"a function lent to a library returned null\");";
    let (succeed, fail) = match returns {
        Type::Void => (format!("{call};\n\t\t\t\treturn true;"), "false"),
        Type::Bytes => (
            format!("var result = {call};{refused}\n\t\t\t\treturn result;"),
            "null",
        ),
        Type::Data(data) if lent::crossed(returns) == Type::Bytes => {
            let mut lines = vec![
                format!("var result = {call};"),
                String::from("var out = new haxe.io.BytesBuffer();"),
            ];
            lines.extend(data::haxe_write(data, "result", "out", 0));
            lines.push(String::from("return out.getBytes();"));
            (lines.join("\n").replace('\n', "\n\t\t\t\t"), "null")
        }
        // Null is how the guard reports a throw, so it is not a valid result.
        Type::Str => (
            format!("var result = {call};{refused}\n\t\t\t\treturn result;"),
            "null",
        ),
        _ => (format!("return {call};"), "null"),
    };
    let returned = match lent::crossed(returns) {
        Type::Void => String::from("Bool"),
        Type::Bytes => String::from("haxe.io.Bytes"),
        other => format!("Null<{}>", native_type(&other)),
    };
    format!(
        "var {guarded} = function({declared}):{returned} {{{owned}\n\t\t\ttry {{\n\t\t\t\t{read}{succeed}\n\t\t\t}} catch (error:haxe.Exception) {{\n\t\t\t\thxn.Callback.failed(error);\n\t\t\t\treturn {fail};\n\t\t\t}}\n\t\t}};",
        guarded = lent::guarded(name),
        declared = declared.join(", "),
    )
}

fn method(library: &Library, interface: &Interface, method: &Method) -> String {
    let native = native_class(library);
    let mut guards = Vec::new();
    let mut arguments = Vec::new();
    if has_receiver(method) {
        arguments.push(String::from("hxnRaw"));
    }
    let mut lines = grown(interface, method);
    for param in &method.params {
        let mut name = haxe_param(&param.name);
        if let Some(data) = &param.data
            && param.ty == Type::Bytes
        {
            let (statements, bytes) = written(&name, data);
            lines.extend(statements);
            name = bytes;
        }
        match &param.ty {
            // Null is passed through whether or not the library accepts it:
            // the library rejects it, the same way on every runtime.
            Type::Handle { .. } => arguments.push(format!("{name} == null ? null : {name}.hxnRaw")),
            Type::Lent { params, returns } => {
                guards.push(guard(&name, params, returns));
                arguments.push(lent::guarded(&name));
            }
            _ => arguments.push(name),
        }
    }
    let call = format!(
        "{native}.{}({})",
        local(&interface.name, &method.name),
        arguments.join(", ")
    );
    if method.constructor {
        lines.push(format!("hxnRaw = {call};"));
        if interface.weighed {
            lines.push(String::from("hxnMemory.check();"));
        }
    } else {
        match &method.returns {
            Type::Void => lines.push(format!("{call};")),
            Type::Bytes => match &method.returns_data {
                Some(data) => lines.extend(read(data, &call)),
                None => lines.push(format!("return {call};")),
            },
            Type::Handle {
                interface: of,
                nullable: false,
            } => lines.push(format!("return {of}.hxnAdopt({call});")),
            Type::Handle {
                interface: of,
                nullable: true,
            } => {
                lines.push(format!("var made = {call};"));
                lines.push(format!("return made == null ? null : {of}.hxnAdopt(made);"));
            }
            Type::Future(_) | Type::Stream(_) | Type::Signal(_) => {
                let (kind, carried) = later::of(&method.returns).expect("a result over time");
                let made = format!(
                    "{}.{}({call})",
                    later::class(library, kind),
                    later::maker(carried)
                );
                lines.push(if method.returns_list {
                    format!("return [for (item in {made}) item];")
                } else {
                    format!("return {made};")
                });
            }
            Type::AtomicBool => lines.push(format!("return hxn.AtomicBool.over({call});")),
            _ => lines.push(format!("return {call};")),
        }
    }
    let mut out = docs(&method.docs, "\t");
    out.push_str(&signature(method));
    out.push_str(&body(&guards, &lines, thrown(library, method).as_deref()));
    out
}

fn module(library: &Library, interface: &Interface) -> String {
    let name = &interface.name;
    let native = native_class(library);
    let mut out = String::from(HEADER);
    if interface.namespace {
        out.push_str(&format!("{NAMESPACE_DOCS}class {name} {{"));
        for (index, m) in interface.methods.iter().enumerate() {
            out.push_str(if index == 0 { "\n" } else { "\n\n" });
            out.push_str(method(library, interface, m).trim_end());
        }
        out.push_str("\n}\n");
        return out;
    }
    out.push_str(&docs(&interface.docs, ""));
    let check = if interface.weighed {
        "\t\thxnMemory.check();\n"
    } else {
        ""
    };
    out.push_str(&format!(
        "class {name} {{\n\t@:noCompletion public var hxnRaw:hxn.Handle;\n{meter}\n\t/** An object for a value the library returned. **/\n\t@:noCompletion public static function hxnAdopt(hxnRaw:hxn.Handle):{name} {{\n\t\tvar made:{name} = Type.createEmptyInstance({name});\n\t\tmade.hxnRaw = hxnRaw;\n{check}\t\treturn made;\n\t}}\n",
        meter = meter(library),
    ));
    for m in &interface.methods {
        out.push('\n');
        out.push_str(&method(library, interface, m));
    }
    if interface.shared {
        out.push_str(&format!(
            "\n{SAME_DOCS}\tpublic function same(other:Null<{name}>):Bool\n\t\treturn {native}.{same}(hxnRaw, other == null ? null : other.hxnRaw);\n\n{WEAK_DOCS}\tpublic function weak():{weak_class}\n\t\treturn {weak_class}.hxnAdopt({native}.{weak}(hxnRaw));\n",
            same = local(name, "same"),
            weak = local(name, "weak"),
            weak_class = weak_class(name),
        ));
    }
    out.push_str(&close(
        interface,
        &format!("{native}.{}", local(name, "drop")),
    ));
    out
}

// The weak reference to a shared interface's value, as a class of its own.
fn weak(library: &Library, interface: &Interface) -> String {
    let of = &interface.name;
    let name = weak_class(of);
    format!(
        "{HEADER}/** A reference to a `{of}` that does not keep it alive. **/\nclass {name} {{\n\t@:noCompletion public var hxnRaw:hxn.Handle;\n\n\t@:noCompletion public static function hxnAdopt(hxnRaw:hxn.Handle):{name} {{\n\t\tvar made:{name} = Type.createEmptyInstance({name});\n\t\tmade.hxnRaw = hxnRaw;\n\t\treturn made;\n\t}}\n\n{UPGRADE_DOCS}\tpublic function upgrade():Null<{of}> {{\n\t\tvar made = {native}.{upgrade}(hxnRaw);\n\t\treturn made == null ? null : {of}.hxnAdopt(made);\n\t}}\n\n\t/** Releases the reference now. The collector does it otherwise. **/\n\tpublic function close():Void\n\t\t{native}.{close}(hxnRaw);\n}}\n",
        native = native_class(library),
        upgrade = local(of, "weak_upgrade"),
        close = local(of, "weak_close"),
    )
}

fn thrown_file(library: &Library) -> String {
    format!(
        "{HEADER}/**\n\t{THROWN_DOCS}\n**/\n@:noCompletion class {name} {{\n\tpublic static function typed(error:haxe.Exception):haxe.Exception {{\n\t\tvar data = {native}.thrown_take();\n\t\tif (data == null)\n\t\t\treturn error;\n\t\treturn read(error, new haxe.io.BytesInput(data));\n\t}}\n{read}",
        native = native_class(library),
        name = thrown_class(library),
        read = thrown_read(library),
    )
}

fn memory(library: &Library) -> String {
    format!(
        "{HEADER}/**\n\t{MEMORY_DOCS}\n**/\nclass {name} {{\n\tpublic static final meter = new hxn.Gc.Meter(() -> {native}.memory_held(), () -> {native}.memory_wanted());\n}}\n",
        native = native_class(library),
        name = memory_class(library),
    )
}

// The classes that make an `hxn.Future`, `hxn.Stream` or signal over one the
// library returned, one a kind, and the inbox they are delivered through.
fn later_files(library: &Library) -> Vec<File> {
    let used = later::used(library);
    if used.is_empty() {
        return Vec::new();
    }
    let native = native_class(library);
    let mut files: Vec<File> = used
        .iter()
        .map(|(kind, carried)| maker_file(library, *kind, carried))
        .collect();
    let name = later::inbox_class(library);
    files.push(File {
        path: format!("{name}.hx"),
        text: format!(
            "{HEADER}/** {} **/\nclass {name} {{\n\tpublic static final inbox = new hxn.Inbox(limit -> {native}.outbox_wait(limit), () -> {native}.outbox_next());\n}}\n",
            later::INBOX_DOCS
        ),
    });
    files
}

fn maker_file(library: &Library, kind: Kind, carried: &[Type]) -> File {
    let name = later::class(library, kind);
    let native = native_class(library);
    let local = kind.name();
    let mut out = format!(
        "{HEADER}/** Makes an `hxn.{}` over one the library returned. **/\n@:noCompletion class {name} {{\n",
        kind.title()
    );
    for (index, carried) in carried.iter().enumerate() {
        let take = format!(
            "{native}.{local}_{}_{}(hxnRaw)",
            kind.take(),
            later::suffix(carried)
        );
        let take = match later::value_type(carried) {
            Type::Void => format!("{{\n\t\t\t{take};\n\t\t\thxn.Unit.Nothing;\n\t\t}}"),
            Type::List { interface } => {
                let stream = later::class(library, Kind::Stream);
                format!("[for (item in {stream}.of{interface}({take})) item]")
            }
            Type::Handle {
                interface,
                nullable: false,
            } => format!("{interface}.hxnAdopt({take})"),
            Type::Handle {
                interface,
                nullable: true,
            } => format!(
                "{{\n\t\t\tvar made = {take};\n\t\t\tmade == null ? null : {interface}.hxnAdopt(made);\n\t\t}}"
            ),
            Type::Data(data) if later::crossed(carried) == Type::Bytes => format!(
                "{{\n\t\t\tvar input = new haxe.io.BytesInput({take});\n\t\t\t{};\n\t\t}}",
                data::haxe_read(data)
            ),
            // A value whose Haxe type differs from its wire type: a `UInt`,
            // or an enum.
            Type::Data(data @ (Data::Enum(_) | Data::Number(crate::model::Number::U32))) => {
                format!("({take} : {})", data::haxe_type(data))
            }
            _ => take,
        };
        // A failure with one of the library's error types is thrown as its
        // typed exception.
        let take = if kind != Kind::Signal && !library.errors().is_empty() {
            format!(
                "try {take} catch (error:haxe.Exception) throw {}.typed(error)",
                thrown_class(library)
            )
        } else {
            take
        };
        let parts = match kind {
            Kind::Future => format!(
                "() -> {native}.future_watch(hxnRaw), () -> {native}.future_state(hxnRaw), limit -> {native}.future_wait(hxnRaw, limit), () -> {take}"
            ),
            Kind::Stream => format!(
                "() -> {native}.stream_watch(hxnRaw), limit -> {native}.stream_wait(hxnRaw, limit), () -> {take}"
            ),
            Kind::Signal => format!(
                "() -> {native}.signal_watch(hxnRaw), (seen, limit) -> {native}.signal_wait(hxnRaw, seen, limit), () -> {take}"
            ),
        };
        if index != 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "\tpublic static function {}(hxnRaw:hxn.Handle):hxn.{}<{}>\n\t\treturn {};\n",
            later::maker(carried),
            kind.title(),
            later::haxe_type(carried),
            later::haxe_new(library, kind, &parts)
        ));
    }
    out.push_str("}\n");
    File {
        path: format!("{name}.hx"),
        text: out,
    }
}
