//! The manifest as WebIDL: the same API as the JSON form, in a standard
//! interface definition language. Anything WebIDL has no construct for is an
//! extended attribute named `Hxn...`.
//!
//! | Rust | WebIDL |
//! |---|---|
//! | `()`, `bool`, `String` or `&str`, `Vec<u8>` or `&[u8]` | `undefined`, `boolean`, `USVString`, `Uint8Array` |
//! | `i8` to `u64`, `f32`, `f64` | `byte` to `unsigned long long`, `float`, `double` |
//! | `isize`, `usize` | `[HxnSize]` on a `long` or an `unsigned long` |
//! | an interface, an `Option` of one | its name, with `?` |
//! | `Future<T>`, an `async fn` | `Promise<T>` |
//! | a function returning `Stream<T>` or `Signal<T>` | `[HxnStream]` or `[HxnSignal]` on one returning `T` |
//! | an atomic cell | `[HxnAtomic]` on a `long` or a `boolean` |
//! | a callback parameter | a `callback` named after where it is passed |
//! | a `#[hxn::data]` struct, C-like enum | a `dictionary`, an `enum` of the variant names |
//! | a `#[hxn::data]` tagged enum | a `dictionary` with a `tag` and one member per variant, each its own `dictionary` |
//! | an `Option`, a `Vec` or a map of data | `T?`, `sequence<T>`, `record<USVString, T>` |
//! | a `Result` | `[Throws]`, or `[Throws=E]` where the error is data |
//!
//! A map with integer keys is also a `record`, although WebIDL only has
//! string keys; the JSON form specifies the key's type.

use crate::model::{
    Data, Enumeration, Interface, Library, Method, Number, Param, Receiver, Record, Tagged, Type,
};
use crate::names::{camel, pascal};

// WebIDL's reserved words. A name that matches one is escaped with a
// leading underscore.
const RESERVED: [&str; 32] = [
    "any",
    "async",
    "attribute",
    "boolean",
    "byte",
    "callback",
    "const",
    "constructor",
    "dictionary",
    "double",
    "enum",
    "float",
    "getter",
    "includes",
    "inherit",
    "interface",
    "iterable",
    "long",
    "mixin",
    "namespace",
    "object",
    "octet",
    "optional",
    "or",
    "partial",
    "readonly",
    "sequence",
    "setter",
    "short",
    "static",
    "undefined",
    "unsigned",
];

fn name(name: &str) -> String {
    if RESERVED.contains(&name) {
        format!("_{name}")
    } else {
        name.to_owned()
    }
}

// A size type is as wide as the target machine's word, which WebIDL has no
// type for: it is a `long` marked `HxnSize`.
fn number(number: Number) -> &'static str {
    match number {
        Number::I8 => "byte",
        Number::I16 => "short",
        Number::U8 => "octet",
        Number::U16 => "unsigned short",
        Number::U32 | Number::Usize => "unsigned long",
        Number::Isize => "long",
        Number::U64 => "unsigned long long",
        Number::F32 => "float",
    }
}

fn sized(number: Option<Number>) -> bool {
    matches!(number, Some(Number::Isize | Number::Usize))
}

// Plain data. WebIDL has no nested nullable type, so an option of an option
// is written as a single option.
fn data(data: &Data) -> String {
    match data {
        Data::Bool => String::from("boolean"),
        Data::I32 => String::from("long"),
        Data::I64 => String::from("long long"),
        Data::F64 => String::from("double"),
        Data::Number(narrow) => number(*narrow).to_owned(),
        Data::Text => String::from("USVString"),
        Data::Bytes => String::from("Uint8Array"),
        Data::Enum(name) | Data::Record(name) | Data::Tagged(name) => name.clone(),
        Data::Option(of) => format!("{}?", self::data(of).trim_end_matches('?')),
        Data::List(of) => format!("sequence<{}>", self::data(of)),
        Data::Boxed(of) => self::data(of),
        Data::Map { value, .. } => format!("record<USVString, {}>", self::data(value)),
    }
}

// The members of a dictionary, one per field.
fn members(fields: &[crate::model::Field]) -> String {
    let mut out = String::new();
    for field in fields {
        out.push_str(&comment(&field.docs, "  "));
        // An optional field is a member that is not `required`.
        let required = if matches!(field.ty, Data::Option(_)) {
            ""
        } else {
            "required "
        };
        out.push_str(&format!(
            "  {required}{} {};\n",
            data(&field.ty),
            name(&field.name)
        ));
    }
    out
}

// WebIDL has no tagged union type, so a tagged enum is a dictionary: `tag`
// names the variant, and the member of that name holds its fields.
fn tagged(tagged: &Tagged) -> String {
    let own = &tagged.name;
    let tags: Vec<String> = tagged
        .cases
        .iter()
        .map(|case| format!("\"{}\"", case.name))
        .collect();
    let mut out = format!("enum {own}Tag {{ {} }};\n", tags.join(", "));
    for case in tagged.cases.iter().filter(|case| !case.fields.is_empty()) {
        out.push_str(&format!(
            "\n{}dictionary {own}{} {{\n{}}};\n",
            comment(&case.docs, ""),
            case.name,
            members(&case.fields)
        ));
    }
    out.push_str(&format!(
        "\n{}[HxnTagged] dictionary {own} {{\n  required {own}Tag tag;\n",
        comment(&tagged.docs, "")
    ));
    for case in tagged.cases.iter().filter(|case| !case.fields.is_empty()) {
        out.push_str(&format!(
            "  {own}{} {};\n",
            case.name,
            name(&camel(&crate::names::snake(&case.name)))
        ));
    }
    out.push_str("};\n");
    out
}

fn dictionary(record: &Record) -> String {
    let mut out = comment(&record.docs, "");
    out.push_str(&format!("dictionary {} {{\n", record.name));
    out.push_str(&members(&record.fields));
    out.push_str("};\n");
    out
}

// A WebIDL enum has string values only, so each variant's integer value is
// written as a comment.
fn enumeration(known: &Enumeration) -> String {
    let mut out = comment(&known.docs, "");
    out.push_str(&format!("enum {} {{\n", known.name));
    let last = known.variants.len().saturating_sub(1);
    for (index, variant) in known.variants.iter().enumerate() {
        out.push_str(&comment(&variant.docs, "  "));
        let comma = if index == last { "" } else { "," };
        out.push_str(&format!(
            "  \"{}\"{comma} // {}\n",
            variant.name, variant.value
        ));
    }
    out.push_str("};\n");
    out
}

// A type, given the callback name assigned to a callback parameter.
fn ty(ty: &Type, lent: &str) -> String {
    match ty {
        Type::Void => String::from("undefined"),
        Type::Bool => String::from("boolean"),
        Type::I32 => String::from("long"),
        Type::I64 => String::from("long long"),
        Type::F64 => String::from("double"),
        Type::Str => String::from("USVString"),
        Type::Bytes => String::from("Uint8Array"),
        // An atomic cell, a stream or a signal is marked by an extended
        // attribute, placed where the grammar allows one.
        Type::AtomicInt => String::from("long"),
        Type::AtomicBool => String::from("boolean"),
        Type::Future(of) => format!("Promise<{}>", self::ty(of, lent)),
        Type::Stream(of) | Type::Signal(of) => self::ty(of, lent),
        Type::Lent { .. } => lent.to_owned(),
        Type::Data(data) => self::data(data),
        Type::Handle {
            interface,
            nullable,
        } => {
            if *nullable {
                format!("{interface}?")
            } else {
                interface.clone()
            }
        }
    }
}

// The callback name for the parameter `param` of `method`: named after where
// it is passed, since WebIDL has no anonymous function type.
fn callback(interface: &Interface, method: &Method, param: &Param) -> String {
    format!(
        "{}{}{}",
        interface.name,
        pascal(&method.name),
        pascal(&param.name)
    )
}

fn param(interface: &Interface, method: &Method, param: &Param) -> String {
    let lent = callback(interface, method, param);
    let written = match &param.data {
        Some(data) => self::data(data),
        None => param
            .number
            .map_or_else(|| ty(&param.ty, &lent), |narrow| number(narrow).to_owned()),
    };
    let mut notes = Vec::new();
    if matches!(param.ty, Type::AtomicInt | Type::AtomicBool) {
        notes.push("HxnAtomic");
    }
    if sized(param.number) {
        notes.push("HxnSize");
    }
    if param.exclusive {
        notes.push("HxnExclusive");
    }
    if param.kept {
        notes.push("HxnKept");
    }
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!("[{}] ", notes.join(", "))
    };
    format!("{notes}{written} {}", name(&param.name))
}

fn comment(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .map(|line| format!("{indent}// {line}\n").replace(" \n", "\n"))
        .collect()
}

fn operation(interface: &Interface, method: &Method) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|param| self::param(interface, method, param))
        .collect();
    let mut out = comment(&method.docs, "  ");
    // An error type that is data is named, whether the call fails with it or
    // an async function's future is rejected with it.
    let thrown = method
        .error
        .as_ref()
        .map(|error| format!("Throws={}", error.name()));
    let throws = match &thrown {
        Some(thrown) => Some(thrown.as_str()),
        None if method.fallible && !method.asynchronous => Some("Throws"),
        None => None,
    };
    if method.constructor {
        // A constructor that can fail has nowhere else to record it.
        let throws = throws.map_or_else(String::new, |throws| format!("[{throws}] "));
        out.push_str(&format!("  {throws}constructor({});\n", params.join(", ")));
        return out;
    }
    let mut notes = Vec::new();
    notes.extend(throws);
    if method.asynchronous {
        notes.push("HxnAsync");
    }
    if method.blocking {
        notes.push("HxnBlocking");
    }
    // An attribute describing the return value goes on the operation, since
    // a return type cannot carry one.
    match &method.returns {
        Type::AtomicInt | Type::AtomicBool => notes.push("HxnAtomic"),
        Type::Stream(_) => notes.push("HxnStream"),
        Type::Signal(_) => notes.push("HxnSignal"),
        _ => {}
    }
    if sized(method.returns_number) {
        notes.push("HxnSize");
    }
    match method.receiver {
        Receiver::Exclusive => notes.push("HxnMutating"),
        Receiver::Owned => notes.push("HxnConsuming"),
        Receiver::None | Receiver::Shared => {}
    }
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!("[{}] ", notes.join(", "))
    };
    // A namespace's operations are implicitly static.
    let modifier = if method.receiver == Receiver::None && !interface.namespace {
        "static "
    } else {
        ""
    };
    let returns = match &method.returns_data {
        Some(data) => self::data(data),
        None => method.returns_number.map_or_else(
            || ty(&method.returns, ""),
            |narrow| number(narrow).to_owned(),
        ),
    };
    out.push_str(&format!(
        "  {notes}{modifier}{returns} {}({});\n",
        name(&method.name),
        params.join(", ")
    ));
    out
}

fn callbacks(interface: &Interface) -> String {
    let mut out = String::new();
    for method in &interface.methods {
        for param in &method.params {
            let Type::Lent { params, returns } = &param.ty else {
                continue;
            };
            let taken: Vec<String> = params
                .iter()
                .enumerate()
                .map(|(n, taken)| format!("{} a{n}", ty(taken, "")))
                .collect();
            out.push_str(&format!(
                "callback {} = {} ({});\n",
                callback(interface, method, param),
                ty(returns, ""),
                taken.join(", ")
            ));
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn interface(interface: &Interface) -> String {
    let mut out = callbacks(interface);
    out.push_str(&comment(&interface.docs, ""));
    if interface.namespace {
        out.push_str(&format!("namespace {} {{\n", interface.name));
    } else {
        // No `Exposed`: it concerns a browser's global scopes and does not
        // apply to a native library.
        let mut notes = Vec::new();
        if interface.shared {
            notes.push("HxnShared");
        }
        if interface.weighed {
            notes.push("HxnWeighed");
        }
        if !notes.is_empty() {
            out.push_str(&format!("[{}]\n", notes.join(", ")));
        }
        out.push_str(&format!("interface {} {{\n", interface.name));
    }
    let operations: Vec<String> = interface
        .methods
        .iter()
        .map(|method| operation(interface, method))
        .collect();
    out.push_str(&operations.join("\n"));
    out.push_str("};\n");
    out
}

/// The manifest of `library`, as WebIDL. The same input gives the same text.
pub fn webidl(library: &Library) -> String {
    let mut interfaces: Vec<String> = library.enums.iter().map(enumeration).collect();
    interfaces.extend(library.records.iter().map(dictionary));
    interfaces.extend(library.tagged.iter().map(tagged));
    interfaces.extend(library.interfaces.iter().map(interface));
    format!(
        "// Generated by hxn-gen from the library's source. Do not edit.\n// The API of `{}`, version 1 of hxn's manifest, as WebIDL. An attribute\n// named Hxn... is hxn's: WebIDL has no word for what it marks.\n\n{}",
        library.name,
        interfaces.join("\n")
    )
}
