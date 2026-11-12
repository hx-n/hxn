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
use crate::model::{Api, Function, Int, Param, Status, Ty};

const KEYWORDS: [&str; 39] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "gen",
];

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
        || function.status != Status::None
        || function
            .params
            .iter()
            .any(|param| fallible_param(&param.ty))
        || matches!(function.returns, Ty::Enum(_) | Ty::Object(_))
}

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
                before.push(format!("let {name} = text({name})?;"));
                passed.push(format!("{name}.as_ptr()"));
            }
            Ty::Bytes => passed.push(format!("{name}.as_ptr(), {name}.len()")),
            Ty::Enum(_) => passed.push(format!("{name} as c_int")),
            Ty::Struct(record) => passed.push(format!("ffi::{record}::from(&{name})")),
            Ty::StructRef(record) => {
                before.push(format!("let {name} = ffi::{record}::from({name});"));
                passed.push(format!("&raw const {name}"));
            }
            Ty::Object(_) => passed.push(format!("{name}.raw")),
            Ty::Nullable(of) if matches!(**of, Ty::Object(_)) => {
                passed.push(format!("{name}.map_or(null_mut(), |value| value.raw)"));
            }
            _ => passed.push(name),
        }
    }
    (before, passed)
}

// The expression for the value the safe function returns, from `value`,
// what C returned. Inside a function that can fail when `fallible`.
fn convert_return(function: &Function) -> Vec<String> {
    let c = &function.c;
    let mut lines = Vec::new();
    match function.status {
        Status::Zero => {
            lines.push(format!(
                "if value != 0 {{\n            return Err(format!(\"{c} failed with status {{value}}\"));\n        }}"
            ));
            lines.push(String::from("Ok(())"));
            return lines;
        }
        Status::Negative => {
            lines.push(format!(
                "if value < 0 {{\n            return Err(format!(\"{c} failed with status {{value}}\"));\n        }}"
            ));
        }
        Status::None => {}
    }
    let value = match &function.returns {
        Ty::Void => String::from("value"),
        Ty::Int(Int::Long) => String::from("i64::from(value)"),
        Ty::Int(Int::ULong) => String::from("u64::from(value)"),
        Ty::Int(Int::Size) => String::from("value as u64"),
        Ty::Int(Int::SSize) => String::from("value as i64"),
        Ty::Enum(name) => format!("{name}::from_c(value)?"),
        Ty::Struct(name) => format!("{name}::from(value)"),
        Ty::Object(name) => {
            lines.push(format!(
                "if value.is_null() {{\n            return Err(String::from(\"{c} returned null\"));\n        }}"
            ));
            format!("{name} {{ raw: value }}")
        }
        Ty::Nullable(of) => match &**of {
            Ty::Object(name) => format!("(!value.is_null()).then(|| {name} {{ raw: value }})"),
            _ => {
                let copy = String::from("let copied = unsafe { copy(value) };");
                lines.push(copy);
                if let Some(free) = &function.free {
                    lines.push(format!(
                        "if !value.is_null() {{\n            unsafe {{ ffi::{free}(value.cast_mut()) }};\n        }}"
                    ));
                }
                String::from("copied")
            }
        },
        Ty::Text => {
            lines.push(String::from("let copied = unsafe { copy(value) };"));
            if let Some(free) = &function.free {
                lines.push(format!(
                    "if !value.is_null() {{\n            unsafe {{ ffi::{free}(value.cast_mut()) }};\n        }}"
                ));
            }
            String::from("copied.unwrap_or_default()")
        }
        _ => String::from("value"),
    };
    if fallible(function, false) {
        lines.push(format!("Ok({value})"));
    } else {
        lines.push(value);
    }
    lines
}

// A safe function over `function`. `receiver` is `&mut self` for a method,
// and `marker` the attribute a free function carries.
fn wrapper(function: &Function, receiver: bool, marker: &str) -> String {
    let (before, passed) = convert(&function.params);
    let mut params: Vec<String> = Vec::new();
    if receiver {
        params.push(String::from("&mut self"));
    }
    for param in &function.params {
        params.push(format!("{}: {}", ident(&param.name), rust_param(&param.ty)));
    }
    let mut arguments = Vec::new();
    if receiver {
        arguments.push(String::from("self.raw"));
    }
    arguments.extend(passed);
    let returns = rust_return(&function.returns);
    let returns = if function.status == Status::Zero {
        String::from("()")
    } else {
        returns
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
    let ignore = function.returns == Ty::Void && function.status == Status::None;
    if ignore {
        body.push(format!("{call};"));
    } else {
        body.push(format!("let value = {call};"));
        body.extend(convert_return(function));
    }
    format!(
        "{}{marker}    pub fn {}({}){signature} {{\n        {}\n    }}\n",
        docs(&function.docs, "    "),
        ident(&function.name),
        params.join(", "),
        body.join("\n        ")
    )
}

fn constructor(object: &str, function: &Function) -> String {
    let (before, passed) = convert(&function.params);
    let params: Vec<String> = function
        .params
        .iter()
        .map(|param| format!("{}: {}", ident(&param.name), rust_param(&param.ty)))
        .collect();
    let mut body = before;
    body.push(format!(
        "let raw = unsafe {{ ffi::{}({}) }};",
        function.c,
        passed.join(", ")
    ));
    body.push(format!(
        "if raw.is_null() {{\n            return Err(String::from(\"{} returned null\"));\n        }}",
        function.c
    ));
    body.push(format!("Ok({object} {{ raw }})"));
    format!(
        "{}    pub fn new({}) -> Result<{object}, String> {{\n        {}\n    }}\n",
        docs(&function.docs, "    "),
        params.join(", "),
        body.join("\n        ")
    )
}

// The C parameters `param` is passed as.
fn c_params(param: &Param) -> Vec<String> {
    let name = ident(&param.name);
    match &param.ty {
        Ty::Bytes => vec![
            format!("{name}: *const u8"),
            format!("{name}_length: usize"),
        ],
        other => vec![format!("{name}: {}", c_type(other))],
    }
}

fn declaration(function: &Function, receiver: Option<&str>) -> String {
    let mut params = Vec::new();
    if let Some(object) = receiver {
        params.push(format!("this: *mut {object}"));
    }
    for param in &function.params {
        params.extend(c_params(param));
    }
    // `[HxnStatus]` functions return a C `int`.
    let returns = match (function.status, &function.returns) {
        (Status::Zero, _) => String::from(" -> c_int"),
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
        "// Generated by hxn from {source}. Do not edit: edit the WebIDL, and the next\n// `hxn build` generates this again.\n\n#![allow(clippy::useless_conversion, clippy::unnecessary_cast, clippy::let_and_return, clippy::needless_question_mark, dead_code, unused_imports)]\n\nuse std::ffi::{{CStr, CString, c_char, c_int, c_long, c_ulong}};\nuse std::ptr::null_mut;\n"
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
                format!("        pub fn {free}(this: *mut {});\n", object.name),
            );
        }
        if let Some(made) = &object.constructor {
            let mut function = made.clone();
            function.returns = Ty::Object(object.name.clone());
            declare(&mut out, &made.c, declaration(&function, None));
        }
        for method in &object.methods {
            declare(&mut out, &method.c, declaration(method, Some(&object.name)));
        }
        for function in &object.statics {
            declare(&mut out, &function.c, declaration(function, None));
        }
    }
    for function in &api.functions {
        declare(&mut out, &function.c, declaration(function, None));
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
    out.push_str("\n// Text as C takes it. Text with a NUL in it cannot be passed.\nfn text(value: &str) -> Result<CString, String> {\n    CString::new(value).map_err(|_| String::from(\"text with a NUL character cannot be passed to C\"))\n}\n\n// A copy of the text C returned, or `None` for a null pointer.\n//\n// # Safety\n// `value` is null or points to text ending in a NUL.\nunsafe fn copy(value: *const c_char) -> Option<String> {\n    if value.is_null() {\n        return None;\n    }\n    Some(unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned())\n}\n");

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
            "\n{}#[hxn::interface]\npub struct {name} {{\n    raw: *mut ffi::{name},\n}}\n\n// Only moved between threads, never shared: every method takes `&mut self`.\nunsafe impl Send for {name} {{}}\n",
            docs(&object.docs, ""),
        ));
        if let Some(free) = &object.free {
            out.push_str(&format!(
                "\nimpl Drop for {name} {{\n    fn drop(&mut self) {{\n        unsafe {{ ffi::{free}(self.raw) }};\n    }}\n}}\n"
            ));
        }
        let mut members = Vec::new();
        if let Some(made) = &object.constructor {
            members.push(constructor(name, made));
        }
        for method in &object.methods {
            members.push(wrapper(method, true, ""));
        }
        for function in &object.statics {
            members.push(wrapper(function, false, ""));
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
            let text = wrapper(function, false, &marker);
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
