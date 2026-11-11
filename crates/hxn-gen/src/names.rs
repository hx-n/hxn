//! The names generated code agrees on across its files.

use crate::model::{Interface, Library, Method};

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
pub fn local(interface: &Interface, method: &str) -> String {
    format!("{}_{}", snake(&interface.name), method)
}

/// The symbol a method is exported as: `counter_counter_add`.
pub fn export(library: &Library, interface: &Interface, method: &str) -> String {
    format!("{}_{}", library.name, local(interface, method))
}

/// The symbol that frees any of the library's handles.
pub fn release(library: &Library) -> String {
    format!("{}_release", library.name)
}

/// What HashLink knows an interface's values as.
pub fn abstract_name(library: &Library, interface: &Interface) -> String {
    format!("{}_{}", library.name, snake(&interface.name))
}

/// Whether `method` is called on a value, as opposed to the type.
pub fn has_receiver(method: &Method) -> bool {
    method.receiver != crate::model::Receiver::None
}
