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

/// A method's name within its library: `counter_add` for `Counter::add`.
/// The Haxe side declares its primitive under this name.
pub fn local(interface: &str, method: &str) -> String {
    format!("{}_{}", snake(interface), method)
}

/// The symbol a method is exported as: `counter_counter_add`.
pub fn export(library: &Library, interface: &str, method: &str) -> String {
    format!("{}_{}", library.name, local(interface, method))
}

/// The symbol that frees any of the library's handles.
pub fn release(library: &Library) -> String {
    format!("{}_release", library.name)
}

/// The HashLink abstract type name for an interface's values.
pub fn abstract_name(library: &Library, interface: &str) -> String {
    format!("{}_{}", library.name, snake(interface))
}

/// The HashLink abstract type name for a weak reference to a shared
/// interface's value.
pub fn weak_abstract(library: &Library, interface: &str) -> String {
    format!("{}_weak", abstract_name(library, interface))
}

/// The Haxe class of a weak reference to a shared interface's value.
pub fn weak_class(interface: &str) -> String {
    format!("{interface}Weak")
}

/// The HashLink abstract type name for an atomic cell the library returned.
pub fn cell_abstract(library: &Library) -> String {
    format!("{}_atomic", library.name)
}

/// The symbol that returns the address of an atomic cell the library
/// returned.
pub fn cell_export(library: &Library) -> String {
    format!("{}_atomic_cell", library.name)
}

/// The symbol that returns the total bytes the library's values declare.
pub fn held_export(library: &Library) -> String {
    format!("{}_memory_held", library.name)
}

/// The symbol that reports whether the library asked for a collection.
pub fn wanted_export(library: &Library) -> String {
    format!("{}_memory_wanted", library.name)
}

/// The hxcpp class that owns a returned atomic cell: `HxnCounterCell`.
pub fn cell_class(library: &Library) -> String {
    helper_class(library, "Cell")
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
    format!("{}_thrown_take", library.name)
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
