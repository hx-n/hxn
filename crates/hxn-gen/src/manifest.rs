//! The manifest: what a library exposes, in a form that is committed and
//! reviewed. A change to it is a change to the library's ABI.

use crate::model::{Interface, Library, Method, Receiver};

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for letter in text.chars() {
        match letter {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn docs(lines: &[String], indent: &str) -> String {
    if lines.is_empty() {
        return String::new();
    }
    format!("{indent}\"docs\": {},\n", quoted(&lines.join("\n")))
}

fn method(method: &Method) -> String {
    let kind = match (method.receiver, method.constructor) {
        (_, true) => "constructor",
        (Receiver::None, false) => "static",
        (Receiver::Shared, false) => "method",
        (Receiver::Exclusive, false) => "mutating",
        (Receiver::Owned, false) => "consuming",
    };
    let params: Vec<String> = method
        .params
        .iter()
        .map(|param| {
            // Said only for an interface borrowed as `&mut`.
            let borrow = if param.exclusive {
                ", \"borrow\": \"exclusive\""
            } else {
                ""
            };
            format!(
                "{{ \"name\": {}, \"type\": {}{borrow} }}",
                quoted(&param.name),
                quoted(&param.type_name())
            )
        })
        .collect();
    let mut out = String::from("        {\n");
    out.push_str(&format!("          \"name\": {},\n", quoted(&method.name)));
    out.push_str(&docs(&method.docs, "          "));
    out.push_str(&format!("          \"kind\": {},\n", quoted(kind)));
    out.push_str(&format!("          \"params\": [{}],\n", params.join(", ")));
    out.push_str(&format!(
        "          \"returns\": {},\n",
        quoted(&method.returns_name())
    ));
    out.push_str(&format!("          \"fallible\": {},\n", method.fallible));
    out.push_str(&format!("          \"blocking\": {}\n", method.blocking));
    out.push_str("        }");
    out
}

fn interface(interface: &Interface) -> String {
    let methods: Vec<String> = interface.methods.iter().map(method).collect();
    let mut out = String::from("    {\n");
    out.push_str(&format!("      \"name\": {},\n", quoted(&interface.name)));
    out.push_str(&docs(&interface.docs, "      "));
    // Each said only when it is.
    if interface.namespace {
        out.push_str("      \"namespace\": true,\n");
    }
    if interface.shared {
        out.push_str("      \"shared\": true,\n");
    }
    if interface.weighed {
        out.push_str("      \"weighed\": true,\n");
    }
    if methods.is_empty() {
        out.push_str("      \"methods\": []\n");
    } else {
        out.push_str(&format!(
            "      \"methods\": [\n{}\n      ]\n",
            methods.join(",\n")
        ));
    }
    out.push_str("    }");
    out
}

/// The manifest of `library`, as JSON. The same input gives the same text.
pub fn manifest(library: &Library) -> String {
    let interfaces: Vec<String> = library.interfaces.iter().map(interface).collect();
    let mut out = String::from("{\n");
    out.push_str("  \"hxn\": 1,\n");
    out.push_str(&format!("  \"library\": {},\n", quoted(&library.name)));
    if interfaces.is_empty() {
        out.push_str("  \"interfaces\": []\n");
    } else {
        out.push_str(&format!(
            "  \"interfaces\": [\n{}\n  ]\n",
            interfaces.join(",\n")
        ));
    }
    out.push_str("}\n");
    out
}
