//! The Rust bindings for an [`Api`]: `extern "C"` declarations, and a safe
//! API over them marked with hxn's attributes.
//!
//! - An object is a struct that owns its C pointer and frees it in `Drop`.
//!   Its methods take `&mut self`, so at most one call runs on an object at
//!   a time: a C library is not assumed to be thread-safe. It is `Send`, so
//!   the garbage collector can free it from its own thread.
//! - A function that can fail returns `Result<_, String>`. It can fail when
//!   it takes text (which may contain a NUL), converts a platform-sized
//!   integer, returns an enum (the C value may name no variant) or an object
//!   (the pointer may be null), or has `[HxnStatus]`.
//! - Returned text is copied. Invalid UTF-8 is replaced, not refused.

use std::collections::BTreeSet;

use crate::idl::namespace;
use crate::model::{Api, Callback, Function, Int, Param, Role, Status, Ty};

// Rust's keywords, reserved words among them.
const KEYWORDS: [&str; 52] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "union", "unsafe", "unsized", "use", "virtual", "where", "while",
];

// Methods every hxn class has already.
const METHODS: [&str; 3] = ["close", "same", "weak"];

// A name usable as a method's.
fn method_name(name: &str) -> String {
    if METHODS.contains(&name) {
        format!("{name}_c")
    } else {
        ident(name)
    }
}

// Names the adapters and this generator use for their own variables.
const TAKEN: [&str; 5] = ["this", "out", "err", "raw", "status"];

// A name usable as a Rust identifier and as an hxn parameter.
fn ident(name: &str) -> String {
    if KEYWORDS.contains(&name) || TAKEN.contains(&name) {
        format!("{name}_value")
    } else {
        name.to_owned()
    }
}

// What the C function takes or returns for `ty`.
fn c_type(ty: &Ty) -> String {
    match ty {
        Ty::Void => String::from("()"),
        Ty::Bool => String::from("bool"),
        Ty::Int(int) => String::from(match int {
            Int::I8 => "i8",
            Int::U8 => "u8",
            Int::I16 => "i16",
            Int::U16 => "u16",
            Int::I32 => "i32",
            Int::U32 => "u32",
            Int::I64 => "i64",
            Int::U64 => "u64",
            Int::Long => "c_long",
            Int::ULong => "c_ulong",
            Int::Size => "usize",
            Int::SSize => "isize",
        }),
        Ty::F32 => String::from("f32"),
        Ty::F64 => String::from("f64"),
        Ty::Text | Ty::Nullable(_) if matches!(ty.inner(), Ty::Text) => {
            String::from("*const c_char")
        }
        Ty::Bytes => String::from("*const u8"),
        Ty::Enum(_) => String::from("c_int"),
        Ty::Struct(name) => format!("ffi::{name}"),
        Ty::StructRef(name) => format!("*const ffi::{name}"),
        Ty::Object(name) => format!("*mut ffi::{name}"),
        Ty::Nullable(of) => c_type(of),
        Ty::Callback(_) => String::from("*mut c_void"),
        Ty::Text => unreachable!(),
    }
}

impl Ty {
    fn inner(&self) -> &Ty {
        match self {
            Self::Nullable(of) => of,
            other => other,
        }
    }
}

// What the safe function takes for `ty`.
fn rust_param(ty: &Ty) -> String {
    match ty {
        Ty::Int(Int::Long | Int::SSize) => String::from("i64"),
        Ty::Int(Int::ULong | Int::Size) => String::from("u64"),
        Ty::Text => String::from("&str"),
        Ty::Bytes => String::from("&[u8]"),
        Ty::Enum(name) | Ty::Struct(name) => name.clone(),
        Ty::StructRef(name) => format!("&{name}"),
        Ty::Object(name) => format!("&mut {name}"),
        Ty::Nullable(of) => format!("Option<{}>", rust_param(of)),
        other => c_type(other),
    }
}

// What the safe function returns for `ty`, less any `Result`.
fn rust_return(ty: &Ty) -> String {
    match ty {
        Ty::Void => String::from("()"),
        Ty::Text => String::from("String"),
        Ty::Nullable(of) => format!("Option<{}>", rust_return(of)),
        Ty::Object(name) | Ty::Enum(name) | Ty::Struct(name) => name.clone(),
        other => rust_param(other),
    }
}

fn fallible_param(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Text | Ty::Int(Int::Long | Int::ULong | Int::Size | Int::SSize)
    )
}

fn fallible(function: &Function, constructor: bool) -> bool {
    constructor
        || lends(function)
        || function.status != Status::None
        || function
            .params
            .iter()
            .any(|param| param.role == Role::In && fallible_param(&param.ty))
        || matches!(function.returns, Ty::Enum(_) | Ty::Object(_))
}

// Whether the function takes a Haxe function, which makes its error the
// one that function fails with.
fn lends(function: &Function) -> bool {
    function
        .params
        .iter()
        .any(|param| matches!(param.ty, Ty::Callback(_)))
}

fn callback<'a>(api: &'a Api, name: &str) -> &'a Callback {
    api.callbacks
        .iter()
        .find(|known| known.name == name)
        .expect("a callback the WebIDL declares")
}

// The callback's name in the functions generated for it: `visitor`.
fn callback_symbol(name: &str) -> String {
    let mut out = String::new();
    for (at, letter) in name.chars().enumerate() {
        if letter.is_ascii_uppercase() {
            if at > 0 {
                out.push('_');
            }
            out.push(letter.to_ascii_lowercase());
        } else {
            out.push(letter);
        }
    }
    out
}

// The function pointer type C takes for `callback`.
fn callback_pointer(callback: &Callback) -> String {
    let mut params: Vec<String> = callback.params.iter().map(|p| c_type(&p.ty)).collect();
    if callback.user_first {
        params.insert(0, String::from("*mut c_void"));
    } else {
        params.push(String::from("*mut c_void"));
    }
    let returns = match callback.returns {
        Ty::Void => String::new(),
        Ty::Bool => String::from(" -> bool"),
        _ => String::from(" -> c_int"),
    };
    format!("unsafe extern \"C\" fn({}){returns}", params.join(", "))
}

// What the Haxe function takes for a callback parameter of type `ty`.
fn callback_value(ty: &Ty) -> String {
    match ty {
        Ty::F32 => String::from("f64"),
        Ty::Text => String::from("&str"),
        other => rust_return(other),
    }
}

// `impl FnMut(i32) -> Result<i32, E>`, the Haxe function lent for `callback`.
fn callback_bound(callback: &Callback) -> String {
    let params: Vec<String> = callback
        .params
        .iter()
        .map(|p| callback_value(&p.ty))
        .collect();
    format!(
        "FnMut({}) -> Result<{}, E>",
        params.join(", "),
        rust_return(&callback.returns)
    )
}

// The trampoline C calls for `callback`, which calls the Haxe function in
// the user data. A failure or a panic is kept for after C returns, and C is
// told to stop, or not called back again where it cannot be told.
fn trampoline(callback: &Callback) -> String {
    let symbol = callback_symbol(&callback.name);
    let bound = callback_bound(callback);
    let mut params: Vec<String> = callback
        .params
        .iter()
        .map(|p| format!("{}: {}", ident(&p.name), c_type(&p.ty)))
        .collect();
    if callback.user_first {
        params.insert(0, String::from("user: *mut c_void"));
    } else {
        params.push(String::from("user: *mut c_void"));
    }
    let (returns, stop) = match callback.returns {
        Ty::Void => (String::new(), String::new()),
        Ty::Bool => (
            String::from(" -> bool"),
            format!(" {}", callback.stop.unwrap_or(0) != 0),
        ),
        _ => (
            String::from(" -> c_int"),
            format!(" {}", callback.stop.unwrap_or(1)),
        ),
    };
    let mut converted = Vec::new();
    let mut passed = Vec::new();
    for param in &callback.params {
        let name = ident(&param.name);
        match &param.ty {
            Ty::F32 => passed.push(format!("f64::from({name})")),
            Ty::Text => {
                converted.push(format!(
                    "let {name} = if {name}.is_null() {{\n        std::borrow::Cow::Borrowed(\"\")\n    }} else {{\n        unsafe {{ CStr::from_ptr({name}) }}.to_string_lossy()\n    }};"
                ));
                passed.push(format!("&{name}"));
            }
            Ty::Enum(known) => {
                converted.push(format!(
                    "let {name} = match {known}::from_c({name}) {{\n        Ok(value) => value,\n        Err(error) => {{\n            lent.failed = Some(E::from(error));\n            return{stop};\n        }}\n    }};"
                ));
                passed.push(name);
            }
            Ty::Struct(record) => passed.push(format!("{record}::from({name})")),
            _ => passed.push(name),
        }
    }
    let (ok, value) = match callback.returns {
        Ty::Void => ("()", String::from("()")),
        _ => ("value", String::from("value")),
    };
    format!(
        "\n// Called by C for a `{name}`, with the Haxe function in `user`.\nunsafe extern \"C\" fn hxn_{symbol}_call<F: {bound}, E: From<String>>({params}){returns} {{\n    let lent = unsafe {{ &mut *user.cast::<Lent<F, E>>() }};\n    if lent.failed.is_some() || lent.panic.is_some() {{\n        return{stop};\n    }}\n    {converted}match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (lent.f)({passed}))) {{\n        Ok(Ok({ok})) => {value},\n        Ok(Err(error)) => {{\n            lent.failed = Some(error);\n            {stop_value}\n        }}\n        Err(panic) => {{\n            lent.panic = Some(panic);\n            {stop_value}\n        }}\n    }}\n}}\n\n// The trampoline for the Haxe function `lent` holds, whose type has no name.\nfn hxn_{symbol}_for<F: {bound}, E: From<String>>(_: &Lent<F, E>) -> {pointer} {{\n    hxn_{symbol}_call::<F, E>\n}}\n",
        name = callback.name,
        params = params.join(", "),
        converted = converted
            .iter()
            .map(|line| format!("{line}\n    "))
            .collect::<String>(),
        passed = passed.join(", "),
        stop_value = if stop.is_empty() {
            String::from("()")
        } else {
            stop.trim().to_owned()
        },
        pointer = callback_pointer(callback),
    )
}

const LENT: &str = "\n// A Haxe function lent to C for one call, and what went wrong in it. C\n// cannot be unwound through, so a failure or a panic is kept here and raised\n// once C has returned.\nstruct Lent<F, E> {\n    f: F,\n    failed: Option<E>,\n    panic: Option<Box<dyn std::any::Any + Send>>,\n}\n\nimpl<F, E> Lent<F, E> {\n    fn new(f: F) -> Self {\n        Self {\n            f,\n            failed: None,\n            panic: None,\n        }\n    }\n\n    // What C is passed as the user data.\n    fn user(&mut self) -> *mut c_void {\n        (self as *mut Self).cast()\n    }\n\n    // The Haxe function's failure, or its panic resumed.\n    fn finish(self) -> Result<(), E> {\n        if let Some(panic) = self.panic {\n            std::panic::resume_unwind(panic);\n        }\n        self.failed.map_or(Ok(()), Err)\n    }\n}\n";

fn docs(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                format!("{indent}///\n")
            } else {
                format!("{indent}/// {line}\n")
            }
        })
        .collect()
}

// The statements that make each argument what C takes, and the arguments.
fn convert(params: &[Param]) -> (Vec<String>, Vec<String>) {
    let mut before = Vec::new();
    let mut passed = Vec::new();
    for param in params {
        let name = ident(&param.name);
        match param.role {
            Role::In => {}
            Role::Null => {
                passed.push(String::from("std::ptr::null_mut()"));
                continue;
            }
            // A place for C to write into, read after the call.
            Role::Out => {
                let (init, pointer) = match param.ty.inner() {
                    Ty::Object(_) => (String::from("null_mut()"), format!("&raw mut {name}")),
                    Ty::Text => (String::from("std::ptr::null()"), format!("&raw mut {name}")),
                    Ty::Struct(record) => (
                        format!("std::mem::MaybeUninit::<ffi::{record}>::uninit()"),
                        format!("{name}.as_mut_ptr()"),
                    ),
                    other => (
                        format!("{}::default()", c_type(other)),
                        format!("&raw mut {name}"),
                    ),
                };
                before.push(format!("let mut {name} = {init};"));
                passed.push(pointer);
                continue;
            }
        }
        match &param.ty {
            Ty::Int(int @ (Int::Long | Int::ULong | Int::Size | Int::SSize)) => {
                before.push(format!(
                    "let {name} = {}::try_from({name}).map_err(|_| format!(\"{name}: {{}} does not fit a C {}\", {name}))?;",
                    c_type(&param.ty),
                    int.alias().unwrap_or_default().replace('_', " ")
                ));
                passed.push(name);
            }
            Ty::Text => {
                before.push(format!("let {name} = hxn_text({name})?;"));
                passed.push(format!("{name}.as_ptr()"));
            }
            Ty::Bytes => passed.push(format!("{name}.as_ptr(), {name}.len()")),
            Ty::Enum(_) => passed.push(format!("{name} as c_int")),
            Ty::Struct(record) => passed.push(format!("ffi::{record}::from(&{name})")),
            Ty::StructRef(record) => {
                before.push(format!("let {name} = ffi::{record}::from({name});"));
                passed.push(format!("&raw const {name}"));
            }
            Ty::Callback(_) => passed.push(format!("Some({name}_call), {name}.user()")),
            Ty::Object(_) => passed.push(format!("{name}.raw")),
            Ty::Nullable(of) if matches!(**of, Ty::Object(_)) => {
                passed.push(format!("{name}.map_or(null_mut(), |value| value.raw)"));
            }
            _ => passed.push(name),
        }
    }
    (before, passed)
}

// Fails the call when `status`, the C function's return value, reports a
// failure.
fn status_check(function: &Function, status: &str) -> Option<String> {
    let c = &function.c;
    let failed = match function.status {
        Status::None => return None,
        Status::Zero => format!("{status} != 0"),
        Status::Negative => format!("{status} < 0"),
    };
    Some(format!(
        "if {failed} {{\n            return Err(format!(\"{c} failed with status {{{status}}}\"));\n        }}"
    ))
}

// The statements that make `value`, what C gave, the safe function's
// result, ending with the result.
fn convert_return(function: &Function, value: &str) -> Vec<String> {
    let c = &function.c;
    let mut lines = Vec::new();
    let result = match &function.returns {
        Ty::Void => String::from("()"),
        Ty::Int(Int::Long) => format!("i64::from({value})"),
        Ty::Int(Int::ULong) => format!("u64::from({value})"),
        Ty::Int(Int::Size) => format!("{value} as u64"),
        Ty::Int(Int::SSize) => format!("{value} as i64"),
        Ty::Enum(name) => format!("{name}::from_c({value})?"),
        Ty::Struct(name) => format!("{name}::from({value})"),
        Ty::Object(name) => {
            lines.push(format!(
                "if {value}.is_null() {{\n            return Err(String::from(\"{c} returned null\"));\n        }}"
            ));
            format!("{name} {{ raw: {value}, owned: {} }}", !function.borrowed)
        }
        Ty::Nullable(of) if matches!(**of, Ty::Object(_)) => {
            let Ty::Object(name) = &**of else {
                unreachable!()
            };
            format!(
                "(!{value}.is_null()).then(|| {name} {{ raw: {value}, owned: {} }})",
                !function.borrowed
            )
        }
        Ty::Text | Ty::Nullable(_) => {
            lines.push(format!("let copied = unsafe {{ hxn_copy({value}) }};"));
            if let Some(free) = &function.free {
                lines.push(format!(
                    "if !{value}.is_null() {{\n            unsafe {{ ffi::{free}({value}.cast_mut()) }};\n        }}"
                ));
            }
            if function.returns == Ty::Text {
                String::from("copied.unwrap_or_default()")
            } else {
                String::from("copied")
            }
        }
        _ => value.to_owned(),
    };
    if fallible(function, false) {
        lines.push(format!("Ok({result})"));
    } else {
        lines.push(result);
    }
    lines
}

// How a method takes its object: `&mut self`, or `&self` for one marked
// thread-safe.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiver {
    None,
    Exclusive,
    Shared,
}

// A safe function over `function`, with `marker` the attribute a free
// function carries.
fn wrapper(api: &Api, function: &Function, receiver: Receiver, marker: &str) -> String {
    let (before, passed) = convert(&function.params);
    let mut params: Vec<String> = Vec::new();
    if receiver != Receiver::None {
        params.push(String::from(if receiver == Receiver::Shared {
            "&self"
        } else {
            "&mut self"
        }));
    }
    for param in function.params.iter().filter(|p| p.role == Role::In) {
        let ty = match &param.ty {
            Ty::Callback(name) => format!("impl {}", callback_bound(callback(api, name))),
            other => rust_param(other),
        };
        params.push(format!("{}: {ty}", ident(&param.name)));
    }
    let mut arguments = Vec::new();
    if receiver != Receiver::None {
        arguments.push(String::from("self.raw"));
    }
    arguments.extend(passed);
    let out = function.out();
    let returns = if function.status == Status::Zero && out.is_none() {
        String::from("()")
    } else {
        rust_return(&function.returns)
    };
    let signature = if fallible(function, false) {
        format!(" -> Result<{returns}, String>")
    } else if returns == "()" {
        String::new()
    } else {
        format!(" -> {returns}")
    };
    let mut body: Vec<String> = before;
    let call = format!("unsafe {{ ffi::{}({}) }}", function.c, arguments.join(", "));
    match out {
        Some(out) => {
            let name = ident(&out.name);
            if function.status == Status::None {
                body.push(format!("{call};"));
            } else {
                body.push(format!("let status = {call};"));
            }
            // An object C wrote is owned before the status is checked, so
            // that it is freed when the call failed.
            let mut value = name.clone();
            match out.ty.inner() {
                Ty::Object(object) => {
                    body.push(format!(
                        "let {name} = (!{name}.is_null()).then(|| {object} {{ raw: {name}, owned: true }});"
                    ));
                }
                Ty::Struct(_) => {}
                _ => {}
            }
            body.extend(status_check(function, "status"));
            match (out.ty.inner(), &function.returns) {
                (Ty::Object(_), Ty::Nullable(_)) => body.push(if fallible(function, false) {
                    format!("Ok({name})")
                } else {
                    name.clone()
                }),
                (Ty::Object(_), _) => body.push(format!(
                    "{name}.ok_or_else(|| String::from(\"{} returned null\"))",
                    function.c
                )),
                (Ty::Struct(_), _) => {
                    // Written by C, which reported success.
                    body.push(format!("let {name} = unsafe {{ {name}.assume_init() }};"));
                    value = name.clone();
                    body.extend(convert_return(function, &value));
                }
                _ => body.extend(convert_return(function, &value)),
            }
        }
        None if function.returns == Ty::Void && function.status == Status::None => {
            body.push(format!("{call};"));
            if fallible(function, false) {
                body.push(String::from("Ok(())"));
            }
        }
        None if function.status == Status::Zero => {
            body.push(format!("let status = {call};"));
            body.extend(status_check(function, "status"));
            body.push(String::from("Ok(())"));
        }
        None => {
            body.push(format!("let value = {call};"));
            body.extend(status_check(function, "value"));
            body.extend(convert_return(function, "value"));
        }
    }
    // A function that lends Haxe functions runs inside a closure, so that
    // what a Haxe function failed with is returned ahead of what the call
    // made of it.
    let (generics, signature, body) = if lends(function) {
        let lent: Vec<&Param> = function
            .params
            .iter()
            .filter(|p| matches!(p.ty, Ty::Callback(_)))
            .collect();
        let mut wrapped: Vec<String> = Vec::new();
        for param in &lent {
            let Ty::Callback(name) = &param.ty else {
                unreachable!()
            };
            let local = ident(&param.name);
            wrapped.push(format!("let mut {local} = Lent::new({local});"));
            wrapped.push(format!(
                "let {local}_call = hxn_{}_for(&{local});",
                callback_symbol(name)
            ));
        }
        wrapped.push(format!(
            "let result: Result<{returns}, String> = (|| {{\n            {}\n        }})();",
            body.join("\n            ")
        ));
        for param in &lent {
            wrapped.push(format!("{}.finish()?;", ident(&param.name)));
        }
        wrapped.push(String::from("result.map_err(E::from)"));
        (
            String::from("<E: From<String>>"),
            format!(" -> Result<{returns}, E>"),
            wrapped,
        )
    } else {
        (String::new(), signature, body)
    };
    format!(
        "{}{marker}    pub fn {}{generics}({}){signature} {{\n        {}\n    }}\n",
        docs(&function.docs, "    "),
        method_name(&function.name),
        params.join(", "),
        body.join("\n        ")
    )
}

// The C parameters `param` is passed as.
fn c_params(api: &Api, param: &Param) -> Vec<String> {
    let name = ident(&param.name);
    if let Ty::Callback(known) = &param.ty {
        return vec![
            format!("{name}: Option<{}>", callback_pointer(callback(api, known))),
            format!("{name}_user: *mut c_void"),
        ];
    }
    if param.role != Role::In {
        return vec![format!("{name}: *mut {}", c_type(param.ty.inner()))];
    }
    match &param.ty {
        Ty::Bytes => vec![
            format!("{name}: *const u8"),
            format!("{name}_length: usize"),
        ],
        other => vec![format!("{name}: {}", c_type(other))],
    }
}

fn declaration(api: &Api, function: &Function, receiver: Option<&str>) -> String {
    let mut params = Vec::new();
    if let Some(object) = receiver {
        params.push(format!("this: *mut {object}"));
    }
    for param in &function.params {
        params.extend(c_params(api, param));
    }
    // `[HxnStatus]` functions return a C `int`.
    let returns = match (function.status, &function.returns) {
        (Status::Zero, _) => String::from(" -> c_int"),
        (Status::None, _) if function.out().is_some() => String::new(),
        (_, Ty::Void) => String::new(),
        (_, Ty::Nullable(of)) if matches!(**of, Ty::Text) && function.free.is_some() => {
            String::from(" -> *const c_char")
        }
        (_, ty) => format!(" -> {}", c_type(ty)),
    };
    format!(
        "        pub fn {}({}){returns};\n",
        function.c,
        params.join(", ")
    )
}

/// The Rust bindings for `api`, the text of a module. `source` names the
/// WebIDL file it was generated from.
pub fn bindings(api: &Api, source: &str) -> String {
    let mut out = format!(
        "// Generated by hxn from {source}. Do not edit: edit the WebIDL, and the next\n// `hxn build` generates this again.\n\n#![allow(clippy::useless_conversion, clippy::unnecessary_cast, clippy::let_and_return, clippy::needless_question_mark, clippy::unnecessary_lazy_evaluations, clippy::redundant_closure_call, dead_code, unused_imports)]\n\nuse std::ffi::{{CStr, CString, c_char, c_int, c_long, c_ulong, c_void}};\nuse std::ptr::null_mut;\n"
    );

    // The C declarations.
    out.push_str("\nmod ffi {\n    use super::*;\n");
    for object in &api.objects {
        out.push_str(&format!(
            "\n    #[repr(C)]\n    pub struct {} {{\n        _opaque: [u8; 0],\n    }}\n",
            object.name
        ));
    }
    for record in &api.structs {
        let fields: String = record
            .fields
            .iter()
            .map(|field| {
                format!(
                    "        pub {}: {},\n",
                    ident(&field.name),
                    c_type(&field.ty)
                )
            })
            .collect();
        out.push_str(&format!(
            "\n    #[repr(C)]\n    #[derive(Clone, Copy)]\n    pub struct {} {{\n{fields}    }}\n",
            record.name
        ));
    }
    out.push_str("\n    unsafe extern \"C\" {\n");
    let mut declared = BTreeSet::new();
    let mut declare = |out: &mut String, name: &str, text: String| {
        if declared.insert(name.to_owned()) {
            out.push_str(&text);
        }
    };
    for object in &api.objects {
        if let Some(free) = &object.free {
            declare(
                &mut out,
                free,
                format!(
                    "        pub fn {free}(this: *mut {}){};\n",
                    object.name,
                    if object.free_int { " -> c_int" } else { "" }
                ),
            );
        }
        if let Some(made) = &object.constructor {
            let mut function = made.clone();
            function.returns = Ty::Object(object.name.clone());
            declare(&mut out, &made.c, declaration(api, &function, None));
        }
        for method in &object.methods {
            declare(
                &mut out,
                &method.c,
                declaration(api, method, Some(&object.name)),
            );
        }
        for function in &object.statics {
            declare(&mut out, &function.c, declaration(api, function, None));
        }
    }
    for function in &api.functions {
        declare(&mut out, &function.c, declaration(api, function, None));
    }
    let frees: BTreeSet<&String> = api
        .objects
        .iter()
        .flat_map(|object| object.methods.iter().chain(&object.statics))
        .chain(&api.functions)
        .filter_map(|function| function.free.as_ref())
        .collect();
    for free in frees {
        declare(
            &mut out,
            free,
            format!("        pub fn {free}(text: *mut c_char);\n"),
        );
    }
    out.push_str("    }\n}\n");

    // Helpers.
    if !api.callbacks.is_empty() {
        out.push_str(LENT);
        for known in &api.callbacks {
            out.push_str(&trampoline(known));
        }
    }
    out.push_str("\n// Text as C takes it. Text with a NUL in it cannot be passed.\nfn hxn_text(value: &str) -> Result<CString, String> {\n    CString::new(value).map_err(|_| String::from(\"text with a NUL character cannot be passed to C\"))\n}\n\n// A copy of the text C returned, or `None` for a null pointer.\n//\n// # Safety\n// `value` is null or points to text ending in a NUL.\nunsafe fn hxn_copy(value: *const c_char) -> Option<String> {\n    if value.is_null() {\n        return None;\n    }\n    Some(unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned())\n}\n");

    for known in &api.enums {
        let variants: String = known
            .values
            .iter()
            .map(|(name, value)| format!("    {name} = {value},\n"))
            .collect();
        let read: String = known
            .values
            .iter()
            .map(|(name, value)| format!("            {value} => Ok(Self::{name}),\n"))
            .collect();
        out.push_str(&format!(
            "\n{}#[hxn::data]\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum {name} {{\n{variants}}}\n\nimpl {name} {{\n    fn from_c(value: c_int) -> Result<Self, String> {{\n        match value {{\n{read}            other => Err(format!(\"{{other}} is not a {name}\")),\n        }}\n    }}\n}}\n",
            docs(&known.docs, ""),
            name = known.name,
        ));
    }

    for record in &api.structs {
        let name = &record.name;
        let fields: String = record
            .fields
            .iter()
            .map(|field| {
                format!(
                    "    pub {}: {},\n",
                    ident(&field.name),
                    rust_return(&field.ty)
                )
            })
            .collect();
        let to_c: String = record
            .fields
            .iter()
            .map(|field| {
                let field = ident(&field.name);
                format!("            {field}: value.{field} as _,\n")
            })
            .collect();
        let from_c: String = record
            .fields
            .iter()
            .map(|field| {
                let field = ident(&field.name);
                format!("            {field}: value.{field} as _,\n")
            })
            .collect();
        out.push_str(&format!(
            "\n{}#[hxn::data]\n#[derive(Clone, Copy, Debug, PartialEq)]\npub struct {name} {{\n{fields}}}\n\nimpl From<&{name}> for ffi::{name} {{\n    fn from(value: &{name}) -> Self {{\n        Self {{\n{to_c}        }}\n    }}\n}}\n\nimpl From<ffi::{name}> for {name} {{\n    fn from(value: ffi::{name}) -> Self {{\n        Self {{\n{from_c}        }}\n    }}\n}}\n",
            docs(&record.docs, ""),
        ));
    }

    for object in &api.objects {
        let name = &object.name;
        out.push_str(&format!(
            "\n{}#[hxn::interface]\npub struct {name} {{\n    raw: *mut ffi::{name},\n    // False for an object that belongs to another, which frees it.\n    owned: bool,\n}}\n\n{}",
            docs(&object.docs, ""),
            if object.thread_safe {
                String::from("// The C library allows calls on it from several threads at once.\nunsafe impl Send for ")
                    + name
                    + " {}\nunsafe impl Sync for "
                    + name
                    + " {}\n"
            } else {
                String::from("// Only moved between threads, never shared: every method takes `&mut self`.\nunsafe impl Send for ")
                    + name
                    + " {}\n"
            },
        ));
        if let Some(free) = &object.free {
            out.push_str(&format!(
                "\nimpl Drop for {name} {{\n    fn drop(&mut self) {{\n        if self.owned {{\n            unsafe {{ ffi::{free}(self.raw) }};\n        }}\n    }}\n}}\n"
            ));
        }
        let mut members = Vec::new();
        if let Some(made) = &object.constructor {
            members.push(wrapper(api, made, Receiver::None, ""));
        }
        for method in &object.methods {
            let receiver = if object.thread_safe {
                Receiver::Shared
            } else {
                Receiver::Exclusive
            };
            members.push(wrapper(api, method, receiver, ""));
        }
        for function in &object.statics {
            members.push(wrapper(api, function, Receiver::None, ""));
        }
        if !members.is_empty() {
            out.push_str(&format!(
                "\n#[hxn::methods]\nimpl {name} {{\n{}}}\n",
                members.join("\n")
            ));
        }
    }

    if !api.functions.is_empty() {
        let class = namespace(&api.library);
        let marker = if api.objects.iter().any(|object| object.name == class) {
            // Statics of the object of the same name.
            String::from("#[hxn::function]\n")
        } else {
            format!("#[hxn::function(class = \"{class}\")]\n")
        };
        for function in &api.functions {
            let text = wrapper(api, function, Receiver::None, &marker);
            // A free function is not indented.
            let text: String = text
                .lines()
                .map(|line| line.strip_prefix("    ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n");
            out.push('\n');
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}
