//! Names that must match across the generated files.

use crate::model::{Library, Method, Receiver};

/// `AudioDevice` as `audio_device`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    for (index, letter) in name.chars().enumerate() {
        if letter.is_uppercase() {
            if index != 0 {
                out.push('_');
            }
            out.extend(letter.to_lowercase());
        } else {
            out.push(letter);
        }
    }
    out
}

/// `sample_rate` as `sampleRate`, which is how Haxe spells it.
pub fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for letter in name.chars() {
        if letter == '_' {
            upper = !out.is_empty();
        } else if upper {
            out.extend(letter.to_uppercase());
            upper = false;
        } else {
            out.push(letter);
        }
    }
    out
}

// C and C++ keywords, and the names the C header and the glue give their own
// parameters.
const C_TAKEN: [&str; 93] = [
    "_Bool",
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "auto",
    "bitand",
    "bitor",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "char16_t",
    "char32_t",
    "char8_t",
    "class",
    "co_await",
    "co_return",
    "co_yield",
    "compl",
    "concept",
    "const",
    "const_cast",
    "consteval",
    "constexpr",
    "constinit",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "err",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "not",
    "not_eq",
    "nullptr",
    "operator",
    "or",
    "or_eq",
    "out",
    "private",
    "protected",
    "public",
    "register",
    "reinterpret_cast",
    "requires",
    "restrict",
    "return",
    "self",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "template",
    "this",
    "thread_local",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
];

/// A parameter's name in the C header and the glue: an underscore follows
/// one that C or C++ keeps for itself.
pub fn c_param(name: &str) -> String {
    if C_TAKEN.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

// Haxe's keywords that are not Rust's, which a Rust parameter can be named.
const HAXE_KEYWORDS: [&str; 22] = [
    "case",
    "cast",
    "catch",
    "class",
    "default",
    "extends",
    "function",
    "implements",
    "import",
    "inline",
    "interface",
    "new",
    "null",
    "operator",
    "overload",
    "package",
    "private",
    "public",
    "switch",
    "this",
    "untyped",
    "var",
];

/// A parameter's name in Haxe: camel case, with an underscore after one of
/// Haxe's keywords.
pub fn haxe_param(name: &str) -> String {
    let name = camel(name);
    if HAXE_KEYWORDS.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

/// A method's name within its library: `counter_add` for `Counter::add`.
/// The Haxe side declares its primitive under this name.
pub fn local(interface: &str, method: &str) -> String {
    format!("{}_{}", snake(interface), method)
}

/// What every symbol the library exports starts with: `hxn_counter`. The
/// `hxn_` keeps an export from taking the name of a C function the library
/// links, which `<library>_<interface>_<method>` alone could: `sqlite3_value_subtype`.
pub fn symbols(library: &Library) -> String {
    format!("hxn_{}", library.name)
}

/// The symbol a method is exported as: `hxn_counter_counter_add`.
pub fn export(library: &Library, interface: &str, method: &str) -> String {
    format!("{}_{}", symbols(library), local(interface, method))
}

/// The symbol that frees any of the library's handles.
pub fn release(library: &Library) -> String {
    format!("{}_release", symbols(library))
}

/// The HashLink abstract type every handle crosses as: one name for every
/// interface, weak reference, cell and result over time, since a handle
/// checks its own type when used, and a library's Haxe classes pass them
/// all as one `hxn.Handle`.
pub const HANDLE_ABSTRACT: &str = "hxn_handle";

/// The HashLink abstract type of an interface's values.
pub fn abstract_name(_library: &Library, _interface: &str) -> String {
    String::from(HANDLE_ABSTRACT)
}

/// The HashLink abstract type name for a weak reference to a shared
/// interface's value.
pub fn weak_abstract(_library: &Library, _interface: &str) -> String {
    String::from(HANDLE_ABSTRACT)
}

/// The Haxe class of a weak reference to a shared interface's value.
pub fn weak_class(interface: &str) -> String {
    format!("{interface}Weak")
}

/// The HashLink abstract type name for an atomic cell the library returned.
pub fn cell_abstract(_library: &Library) -> String {
    String::from(HANDLE_ABSTRACT)
}

/// The symbol that returns the address of an atomic cell the library
/// returned.
pub fn cell_export(library: &Library) -> String {
    format!("{}_atomic_cell", symbols(library))
}

/// The symbol that returns the total bytes the library's values declare.
pub fn held_export(library: &Library) -> String {
    format!("{}_memory_held", symbols(library))
}

/// The symbol that reports whether the library asked for a collection.
pub fn wanted_export(library: &Library) -> String {
    format!("{}_memory_wanted", symbols(library))
}

/// The hxcpp extern class the glue is called through: `HxnCounterNative`.
pub fn native_class(library: &Library) -> String {
    helper_class(library, "Native")
}

/// The Haxe class that holds the library's memory meter: `HxnCounterMemory`.
pub fn memory_class(library: &Library) -> String {
    helper_class(library, "Memory")
}

/// The Haxe class that builds the typed exception for a failure:
/// `HxnCounterThrown`.
pub fn thrown_class(library: &Library) -> String {
    helper_class(library, "Thrown")
}

/// The symbol that returns the value of the last failure.
pub fn thrown_export(library: &Library) -> String {
    format!("{}_thrown_take", symbols(library))
}

/// The Haxe exception class for the error type `name`:
/// `PantryException` for `PantryError`, `RefusedException` for `Refused`.
pub fn exception_class(name: &str) -> String {
    match name.strip_suffix("Error") {
        Some(stem) if !stem.is_empty() => format!("{stem}Exception"),
        _ => format!("{name}Exception"),
    }
}

/// The Haxe class that serializes and deserializes a tagged enum, since a
/// Haxe enum cannot have methods: `EventWire`.
pub fn wire_class(name: &str) -> String {
    format!("{name}Wire")
}

/// `audio_tools` as a class name: `AudioTools`.
pub fn pascal(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for letter in name.chars() {
        if letter == '_' {
            upper = true;
        } else if upper {
            out.extend(letter.to_uppercase());
            upper = false;
        } else {
            out.push(letter);
        }
    }
    out
}

// A helper class of the binding, named apart from the library's own classes:
// `Hxn<Library><role>`.
fn helper_class(library: &Library, role: &str) -> String {
    format!("Hxn{}{role}", pascal(&library.name))
}

/// Whether `method` is called on a value, as opposed to the type.
pub fn has_receiver(method: &Method) -> bool {
    method.receiver != Receiver::None
}
