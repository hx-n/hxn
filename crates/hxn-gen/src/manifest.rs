//! The manifest: a library's API, in a form that is committed and reviewed.
//! A change to it is a change to the library's ABI.

use crate::model::{Enumeration, Interface, Library, Method, Receiver, Record, Tagged};

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
            // Written only for an interface borrowed as `&mut`.
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
    // An async function's error rejects its future instead of failing the
    // call.
    if method.asynchronous {
        out.push_str("          \"async\": true,\n");
    }
    // Written only when the error type is one of the library's data types.
    if let Some(error) = &method.error {
        out.push_str(&format!(
            "          \"throws\": {},\n",
            quoted(&error.name())
        ));
    }
    out.push_str(&format!(
        "          \"fallible\": {},\n",
        method.fallible && !method.asynchronous
    ));
    out.push_str(&format!("          \"blocking\": {}\n", method.blocking));
    out.push_str("        }");
    out
}

fn interface(interface: &Interface) -> String {
    let methods: Vec<String> = interface.methods.iter().map(method).collect();
    let mut out = String::from("    {\n");
    out.push_str(&format!("      \"name\": {},\n", quoted(&interface.name)));
    out.push_str(&docs(&interface.docs, "      "));
    // Each flag is written only when it is set.
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

fn enumeration(known: &Enumeration) -> String {
    let variants: Vec<String> = known
        .variants
        .iter()
        .map(|variant| {
            format!(
                "{{ \"name\": {}, \"value\": {} }}",
                quoted(&variant.name),
                variant.value
            )
        })
        .collect();
    let mut out = String::from("    {\n");
    out.push_str(&format!("      \"name\": {},\n", quoted(&known.name)));
    out.push_str(&docs(&known.docs, "      "));
    out.push_str(&format!(
        "      \"variants\": [{}]\n    }}",
        variants.join(", ")
    ));
    out
}

fn record(record: &Record) -> String {
    let fields: Vec<String> = record
        .fields
        .iter()
        .map(|field| {
            format!(
                "        {{ \"name\": {}, \"type\": {} }}",
                quoted(&field.name),
                quoted(&field.ty.name())
            )
        })
        .collect();
    let mut out = String::from("    {\n");
    out.push_str(&format!("      \"name\": {},\n", quoted(&record.name)));
    out.push_str(&docs(&record.docs, "      "));
    out.push_str(&format!(
        "      \"fields\": [\n{}\n      ]\n    }}",
        fields.join(",\n")
    ));
    out
}

fn tagged(tagged: &Tagged) -> String {
    let cases: Vec<String> = tagged
        .cases
        .iter()
        .map(|case| {
            let fields: Vec<String> = case
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "{{ \"name\": {}, \"type\": {} }}",
                        quoted(&field.name),
                        quoted(&field.ty.name())
                    )
                })
                .collect();
            format!(
                "        {{ \"name\": {}, \"fields\": [{}] }}",
                quoted(&case.name),
                fields.join(", ")
            )
        })
        .collect();
    let mut out = String::from("    {\n");
    out.push_str(&format!("      \"name\": {},\n", quoted(&tagged.name)));
    out.push_str(&docs(&tagged.docs, "      "));
    out.push_str(&format!(
        "      \"cases\": [\n{}\n      ]\n    }}",
        cases.join(",\n")
    ));
    out
}

/// The manifest of `library`, as JSON. The same input gives the same text.
pub fn manifest(library: &Library) -> String {
    let interfaces: Vec<String> = library.interfaces.iter().map(interface).collect();
    let mut out = String::from("{\n");
    out.push_str("  \"hxn\": 1,\n");
    out.push_str(&format!("  \"library\": {},\n", quoted(&library.name)));
    // Data sections are written only when the library has data types.
    if !library.enums.is_empty() {
        let enums: Vec<String> = library.enums.iter().map(enumeration).collect();
        out.push_str(&format!("  \"enums\": [\n{}\n  ],\n", enums.join(",\n")));
    }
    if !library.records.is_empty() {
        let records: Vec<String> = library.records.iter().map(record).collect();
        out.push_str(&format!(
            "  \"records\": [\n{}\n  ],\n",
            records.join(",\n")
        ));
    }
    if !library.tagged.is_empty() {
        let tagged: Vec<String> = library.tagged.iter().map(tagged).collect();
        out.push_str(&format!("  \"tagged\": [\n{}\n  ],\n", tagged.join(",\n")));
    }
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
