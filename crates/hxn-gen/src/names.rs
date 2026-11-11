//! The names generated code agrees on across its files.

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

/// What HashLink knows an interface's values as.
pub fn abstract_name(library: &Library, interface: &str) -> String {
    format!("{}_{}", library.name, snake(interface))
}

/// What HashLink knows a weak reference to a shared interface's value as.
pub fn weak_abstract(library: &Library, interface: &str) -> String {
    format!("{}_weak", abstract_name(library, interface))
}

/// The Haxe class of a weak reference to a shared interface's value.
pub fn weak_class(interface: &str) -> String {
    format!("{interface}Weak")
}

/// What HashLink knows a cell the library handed over as.
pub fn cell_abstract(library: &Library) -> String {
    format!("{}_atomic", library.name)
}

/// The symbol that gives the address of a cell the library handed over.
pub fn cell_export(library: &Library) -> String {
    format!("{}_atomic_cell", library.name)
}

/// The symbol that gives the bytes the library's values declare.
pub fn held_export(library: &Library) -> String {
    format!("{}_memory_held", library.name)
}

/// The symbol that says whether the library asked for a collection.
pub fn wanted_export(library: &Library) -> String {
    format!("{}_memory_wanted", library.name)
}

/// The hxcpp class that owns such a cell: `HxnCounterCell`.
pub fn cell_class(library: &Library) -> String {
    helper_class(library, "Cell")
}

/// The Haxe class that holds the library's meter: `HxnCounterMemory`.
pub fn memory_class(library: &Library) -> String {
    helper_class(library, "Memory")
}

// A class the binding needs beside the library's own: `Hxn<Library><role>`.
fn helper_class(library: &Library, role: &str) -> String {
    let mut name = String::from("Hxn");
    let mut upper = true;
    for letter in library.name.chars() {
        if letter == '_' {
            upper = true;
        } else if upper {
            name.extend(letter.to_uppercase());
            upper = false;
        } else {
            name.push(letter);
        }
    }
    name.push_str(role);
    name
}

/// Whether `method` is called on a value, as opposed to the type.
pub fn has_receiver(method: &Method) -> bool {
    method.receiver != Receiver::None
}
