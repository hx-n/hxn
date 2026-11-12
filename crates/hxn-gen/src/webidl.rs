//! The manifest as WebIDL: the same API as the JSON form, in the language
//! interfaces are usually specified in. What WebIDL has no word for is an
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
//! | a cell | `[HxnAtomic]` on a `long` or a `boolean` |
//! | a lent function | a `callback` named after where it is passed |
//! | a `Result` | `[Throws]` |

use crate::model::{Interface, Library, Method, Number, Param, Receiver, Type};
use crate::names::pascal;

// The words WebIDL keeps for itself, which a name is told apart from by a
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

// A size is as wide as the machine the library was built for, which WebIDL
// has no type for: it is a `long` marked `HxnSize`.
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

// A type, given the callback a lent function was named.
fn ty(ty: &Type, lent: &str) -> String {
    match ty {
        Type::Void => String::from("undefined"),
        Type::Bool => String::from("boolean"),
        Type::I32 => String::from("long"),
        Type::I64 => String::from("long long"),
        Type::F64 => String::from("double"),
        Type::Str => String::from("USVString"),
        Type::Bytes => String::from("Uint8Array"),
        // What says it is a cell, a stream or a signal is an attribute,
        // written where the grammar has a place for one.
        Type::AtomicInt => String::from("long"),
        Type::AtomicBool => String::from("boolean"),
        Type::Future(of) => format!("Promise<{}>", self::ty(of, lent)),
        Type::Stream(of) | Type::Signal(of) => self::ty(of, lent),
        Type::Lent { .. } => lent.to_owned(),
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

// The callback a function lent as `param` of `method` is: named after
// where it is passed, since WebIDL has no unnamed function type.
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
    let written = param
        .number
        .map_or_else(|| ty(&param.ty, &lent), |narrow| number(narrow).to_owned());
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
    if method.constructor {
        // A constructor that can fail has nowhere else to say so.
        let throws = if method.fallible { "[Throws] " } else { "" };
        out.push_str(&format!("  {throws}constructor({});\n", params.join(", ")));
        return out;
    }
    let mut notes = Vec::new();
    if method.fallible && !method.asynchronous {
        notes.push("Throws");
    }
    if method.asynchronous {
        notes.push("HxnAsync");
    }
    if method.blocking {
        notes.push("HxnBlocking");
    }
    // A result's own attribute goes with the operation's: a return type
    // takes none.
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
    // A namespace's operations are static without saying so.
    let modifier = if method.receiver == Receiver::None && !interface.namespace {
        "static "
    } else {
        ""
    };
    let returns = method.returns_number.map_or_else(
        || ty(&method.returns, ""),
        |narrow| number(narrow).to_owned(),
    );
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
        // No `Exposed`: that is about a browser's globals, and says nothing
        // of a native library.
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
    let interfaces: Vec<String> = library.interfaces.iter().map(interface).collect();
    format!(
        "// Generated by hxn-gen from the library's source. Do not edit.\n// The API of `{}`, version 1 of hxn's manifest, as WebIDL. An attribute\n// named Hxn... is hxn's: WebIDL has no word for what it marks.\n\n{}",
        library.name,
        interfaces.join("\n")
    )
}
