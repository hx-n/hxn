//! Futures, streams and signals: the extra exports a library needs when it
//! returns a `Future`, a `Stream` or a `Signal`.
//!
//! Each is returned to the host type-erased, as a handle to an `AnyFuture`,
//! `AnyStream` or `AnySignal`. The library then exports, once for all its
//! functions, the calls that wait on and watch a handle, one call per kind
//! and carried type to take the value, and its queue of ready results. On
//! the Haxe side one class per kind wraps such a handle in an `hxn.Future`,
//! `hxn.Stream` or `hxn.Signal`.

use crate::data;
use crate::haxe::{File, cpp_extern};
use crate::model::{Data, Library, Type};
use crate::names::{abstract_name, native_class, pascal, snake, thrown_class};
use crate::rust::{held, widened};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Future,
    Stream,
    Signal,
}

impl Kind {
    /// Its name in symbols: `future`.
    fn name(self) -> &'static str {
        match self {
            Self::Future => "future",
            Self::Stream => "stream",
            Self::Signal => "signal",
        }
    }

    /// Its name in Haxe and Rust: `Future`.
    fn title(self) -> &'static str {
        match self {
            Self::Future => "Future",
            Self::Stream => "Stream",
            Self::Signal => "Signal",
        }
    }

    /// The method that takes the carried value.
    fn take(self) -> &'static str {
        match self {
            Self::Future => "take",
            Self::Stream => "next",
            Self::Signal => "get",
        }
    }
}

/// The kind of a future, stream or signal type, and the type it carries.
pub fn of(ty: &Type) -> Option<(Kind, &Type)> {
    match ty {
        Type::Future(carried) => Some((Kind::Future, carried)),
        Type::Stream(carried) => Some((Kind::Stream, carried)),
        Type::Signal(carried) => Some((Kind::Signal, carried)),
        _ => None,
    }
}

/// Each kind the library returns, with every type that kind carries anywhere
/// in the API, in order of first use.
pub fn used(library: &Library) -> Vec<(Kind, Vec<Type>)> {
    fn add(out: &mut Vec<(Kind, Vec<Type>)>, kind: Kind, carried: &Type) {
        let index = match out.iter().position(|(known, _)| *known == kind) {
            Some(index) => index,
            None => {
                out.push((kind, Vec::new()));
                out.len() - 1
            }
        };
        if !out[index].1.contains(carried) {
            out[index].1.push(carried.clone());
        }
        // Taking a batch uses the same stream exports as a direct object list.
        if let Type::List { interface } = value_type(carried) {
            add(
                out,
                Kind::Stream,
                &Type::Handle {
                    interface: interface.clone(),
                    nullable: false,
                },
            );
        }
    }
    let mut out = Vec::new();
    for method in library.interfaces.iter().flat_map(|i| &i.methods) {
        if let Some((kind, carried)) = of(&method.returns) {
            add(&mut out, kind, carried);
        }
    }
    out
}

// A fallible stream's Haxe value excludes its terminal error.
fn value_type(carried: &Type) -> &Type {
    match carried {
        Type::Fallible { value, .. } => value,
        other => other,
    }
}

fn take_call(library: &Library, kind: Kind, carried: &Type) -> String {
    match carried {
        Type::Fallible { error, .. } => format!(
            "this.next_result::<{}, {}>({})",
            rust_type(library, carried),
            data::rust_type(error),
            data::thrower(error)
        ),
        _ => format!("this.{}::<{}>()", kind.take(), rust_type(library, carried)),
    }
}

fn interface(carried: &Type) -> Option<&str> {
    match value_type(carried) {
        Type::Handle { interface, .. } => Some(interface),
        _ => None,
    }
}

// A carried type's name within a symbol: `int`, `text`, `counter`.
fn suffix(carried: &Type) -> String {
    if let Type::Fallible { value, error } = carried {
        return format!("result_{}_{}", suffix(value), error.symbol());
    }
    match value_type(carried) {
        Type::Void => String::from("unit"),
        Type::Bool => String::from("bool"),
        Type::I32 => String::from("int"),
        Type::I64 => String::from("long"),
        Type::F64 => String::from("float"),
        Type::Str => String::from("text"),
        Type::Bytes => String::from("bytes"),
        Type::Handle {
            interface,
            nullable,
        } => {
            if *nullable {
                format!("option_{}", snake(interface))
            } else {
                snake(interface)
            }
        }
        Type::List { interface } => format!("list_{}", snake(interface)),
        Type::Data(data) => data.symbol(),
        _ => unreachable!("not carried by a result over time"),
    }
}

/// An error message if two carried types of one kind would give their take
/// calls the same name.
pub fn clash(library: &Library) -> Option<String> {
    for (kind, carried) in used(library) {
        for (index, one) in carried.iter().enumerate() {
            if let Some(other) = carried[..index]
                .iter()
                .find(|other| suffix(other) == suffix(one))
            {
                return Some(format!(
                    "a {kind} of `{}` and a {kind} of `{}` would be taken by calls of one name: call one of the two types something else",
                    other.name(),
                    one.name(),
                    kind = kind.name(),
                ));
            }
        }
    }
    None
}

// The wire type of a carried value: the type itself, or its data's wire type.
fn crossed(carried: &Type) -> Type {
    match value_type(carried) {
        Type::Data(data) => data.crosses().unwrap_or(Type::Bytes),
        other => other.clone(),
    }
}

// A carried type's name within a Haxe name: `Int`, `Text`, `Counter`.
fn title(carried: &Type) -> String {
    if let Type::Fallible { value, error } = carried {
        return format!("Result{}{}", title(value), pascal(&error.symbol()));
    }
    match value_type(carried) {
        Type::Void => String::from("Unit"),
        Type::Bool => String::from("Bool"),
        Type::I32 => String::from("Int"),
        Type::I64 => String::from("Long"),
        Type::F64 => String::from("Float"),
        Type::Str => String::from("Text"),
        Type::Bytes => String::from("Bytes"),
        Type::Handle {
            interface,
            nullable,
        } => {
            if *nullable {
                format!("Option{interface}")
            } else {
                interface.clone()
            }
        }
        Type::List { interface } => format!("List{interface}"),
        Type::Data(data) => pascal(&data.symbol()),
        _ => unreachable!("not carried by a result over time"),
    }
}

fn haxe_type(carried: &Type) -> String {
    match value_type(carried) {
        Type::Void => String::from("hxn.Unit"),
        Type::Bool => String::from("Bool"),
        Type::I32 => String::from("Int"),
        Type::I64 => String::from("haxe.Int64"),
        Type::F64 => String::from("Float"),
        Type::Str => String::from("String"),
        Type::Bytes => String::from("haxe.io.Bytes"),
        Type::Handle {
            interface,
            nullable,
        } => {
            if *nullable {
                format!("Null<{interface}>")
            } else {
                interface.clone()
            }
        }
        Type::List { interface } => format!("Array<{interface}>"),
        Type::Data(data) => data::haxe_type(data),
        _ => unreachable!("not carried by a result over time"),
    }
}

// The Rust type a library's own future, stream or signal carries.
fn rust_type(library: &Library, carried: &Type) -> String {
    match value_type(carried) {
        Type::Void => String::from("()"),
        Type::Bool => String::from("bool"),
        Type::I32 => String::from("i32"),
        Type::I64 => String::from("i64"),
        Type::F64 => String::from("f64"),
        Type::Str => String::from("String"),
        Type::Bytes => String::from("Vec<u8>"),
        Type::Handle {
            interface,
            nullable,
        } => {
            if *nullable {
                format!("Option<{}>", held(library, interface))
            } else {
                held(library, interface)
            }
        }
        Type::List { interface } => format!("Vec<{}>", held(library, interface)),
        Type::Data(data) => data::rust_type(data),
        _ => unreachable!("not carried by a result over time"),
    }
}

/// The HashLink abstract type name for a kind's handles: `counter_future`.
pub fn handle_abstract(library: &Library, kind: Kind) -> String {
    format!("{}_{}", library.name, kind.name())
}

// `Hxn<Library><role>`, named like the binding's other helper classes.
fn helper(library: &Library, role: &str) -> String {
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

/// The Haxe class that creates a kind's objects: `HxnCounterFuture`.
pub fn class(library: &Library, kind: Kind) -> String {
    helper(library, kind.title())
}

/// The Haxe class that holds the library's inbox: `HxnCounterInbox`.
pub fn inbox_class(library: &Library) -> String {
    helper(library, "Inbox")
}

/// The name of the Haxe function that creates one carrying `carried`: `ofInt`.
pub fn maker(carried: &Type) -> String {
    format!("of{}", title(carried))
}

fn export(library: &Library, kind: Kind, call: &str) -> String {
    format!("{}_{}_{call}", crate::names::symbols(library), kind.name())
}

/// The adapter's imports for these. Empty for a library that uses none, so
/// that its adapter crate does not need to depend on `hxn-future`.
pub fn imports(library: &Library) -> &'static str {
    if used(library).is_empty() {
        ""
    } else {
        "#[allow(unused_imports)]\nuse hxn_future::{AnyFuture, AnySignal, AnyStream, outbox};\n"
    }
}

/// A function's return value as the handle returned to the host, over the C
/// ABI.
pub fn c_result(kind: Kind, call: &str) -> String {
    format!("Ok(Handle::into_raw(Any{}::from({call})))", kind.title())
}

/// The same for HashLink.
pub fn hl_result(kind: Kind, call: &str) -> String {
    format!("Ok(manage(Any{}::from({call})))", kind.title())
}

// The parameter names in a list like `milliseconds: i32, `, all of them ints.
fn names(params: &str) -> Vec<&str> {
    params
        .split(", ")
        .filter_map(|param| param.split(':').next())
        .filter(|param| !param.is_empty())
        .collect()
}

// The calls every handle of a kind supports besides taking its value: name,
// extra parameters, and the expression on `this`.
fn calls(kind: Kind) -> Vec<(&'static str, &'static str, &'static str, bool)> {
    // The last is whether the call waits.
    match kind {
        Kind::Future => vec![
            ("state", "", "this.state() as i32", false),
            (
                "wait",
                "milliseconds: i32, ",
                "this.wait_millis(milliseconds) as i32",
                true,
            ),
            ("watch", "", "this.watch()", false),
        ],
        Kind::Stream => vec![
            (
                "wait",
                "milliseconds: i32, ",
                "this.wait_millis(milliseconds) as i32",
                true,
            ),
            ("watch", "", "this.watch()", false),
        ],
        Kind::Signal => vec![
            (
                "wait",
                "seen: i32, milliseconds: i32, ",
                "this.wait_millis(seen, milliseconds)",
                true,
            ),
            ("watch", "", "this.watch()", false),
        ],
    }
}

/// The exports over the C ABI.
pub fn c_adapter(library: &Library) -> String {
    let used = used(library);
    if used.is_empty() {
        return String::new();
    }
    let name = &crate::names::symbols(library);
    let mut out = format!(
        "\n// The library's one queue of what it has for the host.\n#[unsafe(no_mangle)]\npub extern \"C\" fn {name}_outbox_wait(milliseconds: i32) -> i32 {{\n    i32::from(outbox::wait_millis(milliseconds))\n}}\n\n#[unsafe(no_mangle)]\npub extern \"C\" fn {name}_outbox_next() -> i32 {{\n    outbox::next()\n}}\n"
    );
    for (kind, carried) in &used {
        let any = format!("Any{}", kind.title());
        for (call, params, body, _) in calls(*kind) {
            out.push_str(&format!(
                "\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {export}(this: RawHandle, {params}out: *mut i32, err: *mut Buf) -> Status {{\n    unsafe {{\n        guard(out, err, || {{\n            let this = Handle::<{any}>::from_raw(this)?.borrow()?;\n            Ok({body})\n        }})\n    }}\n}}\n",
                export = export(library, *kind, call),
            ));
        }
        for carried in carried {
            // The erased handle's take method already returns a `Result`.
            let value = take_call(library, *kind, carried);
            let (ty, result) = match value_type(carried) {
                // A unit value is passed as an int that is always 1: taking
                // it reports that the work is done, or fails.
                Type::Void => ("i32", format!("{value}.map(|()| 1)")),
                Type::Bool => ("i32", format!("Ok(i32::from({value}?))")),
                Type::I32 => ("i32", value),
                Type::I64 => ("i64", value),
                Type::F64 => ("f64", value),
                Type::Str | Type::Bytes => ("Buf", format!("Ok(Buf::from({value}?))")),
                Type::Data(data) => c_data(data, &format!("{value}?")),
                Type::List { .. } => (
                    "RawHandle",
                    format!(
                        "let stream = {}; Ok(Handle::into_raw(AnyStream::from(stream)))",
                        crate::rust::list_stream(&format!("{value}?"))
                    ),
                ),
                Type::Handle { nullable: true, .. } => (
                    "RawHandle",
                    format!("Ok({value}?.map_or(null_mut(), Handle::into_raw))"),
                ),
                _ => ("RawHandle", format!("Ok(Handle::into_raw({value}?))")),
            };
            out.push_str(&format!(
                "\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {export}(this: RawHandle, out: *mut {ty}, err: *mut Buf) -> Status {{\n    unsafe {{\n        guard(out, err, || {{\n            let this = Handle::<{any}>::from_raw(this)?.borrow()?;\n            {result}\n        }})\n    }}\n}}\n",
                export = export(
                    library,
                    *kind,
                    &format!("{}_{}", kind.take(), suffix(carried))
                ),
            ));
        }
    }
    out
}

// Data carried by a future, stream or signal, converted to its wire type:
// the type, and the result of the call that takes it.
fn c_data(data: &Data, value: &str) -> (&'static str, String) {
    match data {
        Data::Number(number) => {
            let ty = match number.wire() {
                Type::I64 => "i64",
                Type::F64 => "f64",
                _ => "i32",
            };
            (ty, format!("Ok({})", widened(Some(*number), value)))
        }
        Data::Enum(known) => ("i32", format!("Ok({}(&{value}))", data::writer(known))),
        other => (
            "Buf",
            format!("Ok(Buf::from({}))", data::rust_encoded(other, value)),
        ),
    }
}

/// The same as HashLink primitives.
pub fn hl_adapter(library: &Library) -> String {
    let used = used(library);
    if used.is_empty() {
        return String::new();
    }
    let name = &crate::names::symbols(library);
    let mut out = format!(
        "\n// The library's one queue of what it has for the host.\n#[unsafe(no_mangle)]\npub unsafe extern \"C-unwind\" fn {name}_outbox_wait(milliseconds: i32) -> bool {{\n    unsafe {{ blocking(|| outbox::wait_millis(milliseconds)) }}\n}}\ndefine_prim!(hlp_outbox_wait, {name}_outbox_wait, \"Pi_b\");\n\n#[unsafe(no_mangle)]\npub extern \"C\" fn {name}_outbox_next() -> i32 {{\n    outbox::next()\n}}\ndefine_prim!(hlp_outbox_next, {name}_outbox_next, \"P_i\");\n"
    );
    for (kind, carried) in &used {
        let any = format!("Any{}", kind.title());
        let own = handle_abstract(library, *kind);
        for (call, params, body, waits) in calls(*kind) {
            let letters = "i".repeat(params.matches(": i32").count());
            // The garbage collector may run while a call blocks.
            let body = if waits {
                let (wait, cast) = match body.strip_suffix(" as i32") {
                    Some(wait) => (wait, " as i32"),
                    None => (body, ""),
                };
                format!("blocking(|| {wait}){cast}")
            } else {
                String::from(body)
            };
            let export = export(library, *kind, call);
            let params: String = names(params)
                .iter()
                .map(|name| format!(", {name}: i32"))
                .collect();
            out.push_str(&format!(
                "\n#[unsafe(no_mangle)]\npub unsafe extern \"C-unwind\" fn {export}(this: *mut Managed{params}) -> i32 {{\n    unsafe {{\n        prim(|| {{\n            let this = handle::<{any}>(this)?.borrow()?;\n            Ok({body})\n        }})\n    }}\n}}\ndefine_prim!(hlp_{local}_{call}, {export}, \"PX{own}_{letters}_i\");\n",
                local = kind.name(),
            ));
        }
        for carried in carried {
            let value = take_call(library, *kind, carried);
            let (ty, letter, result) = match value_type(carried) {
                Type::Void => ("bool", String::from("b"), format!("{value}.map(|()| true)")),
                Type::Bool => ("bool", String::from("b"), value),
                Type::I32 => ("i32", String::from("i"), value),
                Type::I64 => ("i64", String::from("l"), value),
                Type::F64 => ("f64", String::from("d"), value),
                Type::Str => (
                    "*mut vbyte",
                    String::from("B"),
                    format!("Ok(utf16(&{value}?))"),
                ),
                // The VM's bytes do not carry their length.
                Type::Bytes => (
                    "*mut vbyte",
                    String::from("B"),
                    format!(
                        "let data = {value}?;\n            if out.is_null() {{\n                return Err(Fault::Null.into());\n            }}\n            *out = i32::try_from(data.len()).map_err(|_| Fault::TooLarge)?;\n            Ok(bytes(&data))"
                    ),
                ),
                Type::Data(data) => match c_data(data, &format!("{value}?")) {
                    ("Buf", _) => (
                        "*mut vbyte",
                        String::from("B"),
                        format!(
                            "let data = {};\n            if out.is_null() {{\n                return Err(Fault::Null.into());\n            }}\n            *out = i32::try_from(data.len()).map_err(|_| Fault::TooLarge)?;\n            Ok(bytes(&data))",
                            data::rust_encoded(data, &format!("{value}?"))
                        ),
                    ),
                    ("i64", result) => ("i64", String::from("l"), result),
                    ("f64", result) => ("f64", String::from("d"), result),
                    (_, result) => ("i32", String::from("i"), result),
                },
                Type::List { .. } => (
                    "*mut Managed",
                    format!("X{}_", handle_abstract(library, Kind::Stream)),
                    format!(
                        "let stream = {}; Ok(manage(AnyStream::from(stream)))",
                        crate::rust::list_stream(&format!("{value}?"))
                    ),
                ),
                _ => (
                    "*mut Managed",
                    format!(
                        "X{}_",
                        abstract_name(library, interface(carried).expect("an interface"))
                    ),
                    if matches!(value_type(carried), Type::Handle { nullable: true, .. }) {
                        format!("Ok({value}?.map_or(null_mut(), manage))")
                    } else {
                        format!("Ok(manage({value}?))")
                    },
                ),
            };
            let call = format!("{}_{}", kind.take(), suffix(carried));
            let export = export(library, *kind, &call);
            let (length, taken) = if crossed(carried) == Type::Bytes {
                (", out: *mut i32", "Ri")
            } else {
                ("", "")
            };
            out.push_str(&format!(
                "\n#[unsafe(no_mangle)]\npub unsafe extern \"C-unwind\" fn {export}(this: *mut Managed{length}) -> {ty} {{\n    unsafe {{\n        prim(|| {{\n            let this = handle::<{any}>(this)?.borrow()?;\n            {result}\n        }})\n    }}\n}}\ndefine_prim!(hlp_{local}_{call}, {export}, \"PX{own}_{taken}_{letter}\");\n",
                local = kind.name(),
            ));
        }
    }
    out
}

/// What the C header declares for these.
pub fn c_header(library: &Library) -> String {
    let used = used(library);
    if used.is_empty() {
        return String::new();
    }
    let name = &crate::names::symbols(library);
    let mut out = format!(
        "\n/* What the library has for the host. `wait` gives 1 once something is queued\n * or 0 when the limit passes; `next` gives the id of a future, stream or\n * signal that has something new, or -1. */\nint32_t {name}_outbox_wait(int32_t milliseconds);\nint32_t {name}_outbox_next(void);\n"
    );
    for (kind, carried) in &used {
        out.push_str(&format!(
            "\n/* A {} the library returned: a handle of its own. */\n",
            kind.name()
        ));
        for (call, params, _, _) in calls(*kind) {
            let params: String = names(params)
                .iter()
                .map(|name| format!("int32_t {name}, "))
                .collect();
            out.push_str(&format!(
                "hxn_status {}(hxn_handle *self, {params}int32_t *out, hxn_buf *err);\n",
                export(library, *kind, call)
            ));
        }
        for carried in carried {
            let ty = match &crossed(carried) {
                Type::Void | Type::Bool | Type::I32 => "int32_t *",
                Type::I64 => "int64_t *",
                Type::F64 => "double *",
                Type::Str | Type::Bytes => "hxn_buf *",
                _ => "hxn_handle **",
            };
            out.push_str(&format!(
                "hxn_status {}(hxn_handle *self, {ty}out, hxn_buf *err);\n",
                export(
                    library,
                    *kind,
                    &format!("{}_{}", kind.take(), suffix(carried))
                )
            ));
        }
    }
    out
}

/// The hxcpp glue for these.
pub fn cpp_glue(library: &Library) -> String {
    let used = used(library);
    if used.is_empty() {
        return String::new();
    }
    let name = &crate::names::symbols(library);
    let mut out = format!(
        "\ninline bool outbox_wait(int milliseconds) {{\n\t// The collector may run while this thread waits.\n\thxn::Blocking zone;\n\treturn {name}_outbox_wait(milliseconds) != 0;\n}}\n\ninline int outbox_next() {{\n\treturn {name}_outbox_next();\n}}\n"
    );
    for (kind, carried) in &used {
        for (call, params, _, waits) in calls(*kind) {
            let names = names(params);
            let declared: String = names.iter().map(|n| format!(", int {n}")).collect();
            let passed: String = names.iter().map(|n| format!("{n}, ")).collect();
            let export = export(library, *kind, call);
            let invoke = format!("{export}((hxn_handle *)self, {passed}&out, &err)");
            let body = if waits {
                // The blocking zone ends before any exception is thrown.
                format!(
                    "\thxn_status status;\n\t{{\n\t\thxn::Blocking zone;\n\t\tstatus = {invoke};\n\t}}\n\thxn::check(status, err);\n"
                )
            } else {
                format!("\thxn::check({invoke}, err);\n")
            };
            out.push_str(&format!(
                "\ninline int {local}_{call}(void *self{declared}) {{\n\tint out = 0;\n\thxn_buf err = HXN_BUF_EMPTY;\n{body}\treturn out;\n}}\n",
                local = kind.name(),
            ));
        }
        for carried in carried {
            let (returns, local, result) = match &crossed(carried) {
                Type::Void | Type::Bool => ("bool ", "int out = 0;", "out != 0"),
                Type::I32 => ("int ", "int out = 0;", "out"),
                Type::I64 => ("cpp::Int64 ", "int64_t out = 0;", "out"),
                Type::Bytes => (
                    "Array<unsigned char> ",
                    "hxn_buf out = HXN_BUF_EMPTY;",
                    "hxn::take_bytes(out)",
                ),
                Type::F64 => ("double ", "double out = 0;", "out"),
                Type::Str => (
                    "::String ",
                    "hxn_buf out = HXN_BUF_EMPTY;",
                    "hxn::take_string(out)",
                ),
                _ => ("void *", "hxn_handle *out = 0;", "out"),
            };
            let call = format!("{}_{}", kind.take(), suffix(carried));
            out.push_str(&format!(
                "\ninline {returns}{local_name}_{call}(void *self) {{\n\t{local}\n\thxn_buf err = HXN_BUF_EMPTY;\n\thxn::check({export}((hxn_handle *)self, &out, &err), err);\n\treturn {result};\n}}\n",
                local_name = kind.name(),
                export = export(library, *kind, &call),
            ));
        }
    }
    out
}

const HEADER: &str = "// Generated by hxn-gen from the library's source. Do not edit.\n";

// The Haxe expression that creates the object wrapping a handle, given the
// functions that watch it, wait on it and take its value.
fn haxe_new(library: &Library, kind: Kind, parts: &str) -> String {
    let inbox = format!("{}.inbox", inbox_class(library));
    match kind {
        Kind::Future => format!("new hxn.Future({inbox}, {parts})"),
        Kind::Stream => format!("new hxn.Stream({inbox}, {parts})"),
        Kind::Signal => format!("hxn.Reactive.follow({inbox}, {parts})"),
    }
}

fn hl_file(library: &Library, kind: Kind, carried: &[Type]) -> File {
    let name = class(library, kind);
    let local = kind.name();
    let raw = format!("{name}Raw");
    let mut out = format!(
        "{HEADER}@:noCompletion typedef {raw} = hl.Abstract<\"{}\">;\n\n@:hlNative(\"{}\")\nprivate class Native {{\n",
        handle_abstract(library, kind),
        library.name
    );
    for (call, params, _, _) in calls(kind) {
        let params: String = names(params)
            .iter()
            .map(|param| format!(", {param}:Int"))
            .collect();
        out.push_str(&format!(
            "\tpublic static function {local}_{call}(self:{raw}{params}):Int\n\t\treturn 0;\n\n"
        ));
    }
    for carried in carried {
        let (ty, nothing) = match &crossed(carried) {
            Type::Void | Type::Bool => (String::from("Bool"), "false"),
            Type::I32 => (String::from("Int"), "0"),
            Type::I64 => (String::from("hl.I64"), "0"),
            Type::F64 => (String::from("Float"), "0."),
            Type::Str | Type::Bytes => (String::from("hl.Bytes"), "null"),
            Type::List { .. } => {
                let stream = class(library, Kind::Stream);
                (format!("{stream}.{stream}Raw"), "null")
            }
            _ => {
                let interface = interface(carried).expect("an interface");
                (format!("{interface}.{interface}Raw"), "null")
            }
        };
        let length = if crossed(carried) == Type::Bytes {
            ", out:hl.Ref<Int>"
        } else {
            ""
        };
        out.push_str(&format!(
            "\tpublic static function {local}_{}_{}(self:{raw}{length}):{ty}\n\t\treturn {nothing};\n\n",
            kind.take(),
            suffix(carried)
        ));
    }
    out.truncate(out.len() - 1);
    out.push_str(&format!(
        "}}\n\n/** Makes an `hxn.{}` over one the library returned. **/\n@:noCompletion class {name} {{\n",
        kind.title()
    ));
    for (index, carried) in carried.iter().enumerate() {
        let take = format!("Native.{local}_{}_{}(hxnRaw)", kind.take(), suffix(carried));
        let take = match value_type(carried) {
            Type::Void => format!("{{\n\t\t\t{take};\n\t\t\thxn.Unit.Nothing;\n\t\t}}"),
            Type::Bytes => format!(
                "{{\n\t\t\tvar outLength = 0;\n\t\t\tvar data = Native.{local}_{}_{}(hxnRaw, outLength);\n\t\t\t@:privateAccess new haxe.io.Bytes(data, outLength);\n\t\t}}",
                kind.take(),
                suffix(carried)
            ),
            Type::Str => format!("@:privateAccess String.fromUCS2({take})"),
            Type::List { interface } => {
                let stream = class(library, Kind::Stream);
                format!("[for (item in {stream}.of{interface}({take})) item]")
            }
            Type::Handle {
                interface,
                nullable: false,
            } => format!("{interface}.hxnAdopt({take})"),
            Type::Handle {
                interface,
                nullable: true,
            } => format!(
                "{{\n\t\t\tvar made = {take};\n\t\t\tmade == null ? null : {interface}.hxnAdopt(made);\n\t\t}}"
            ),
            Type::Data(data) if crossed(carried) == Type::Bytes => format!(
                "{{\n\t\t\tvar outLength = 0;\n\t\t\tvar data = Native.{local}_{}_{}(hxnRaw, outLength);\n\t\t\tvar input = new haxe.io.BytesInput(@:privateAccess new haxe.io.Bytes(data, outLength));\n\t\t\t{};\n\t\t}}",
                kind.take(),
                suffix(carried),
                data::haxe_read(data)
            ),
            // A value whose Haxe type differs from its wire type: a `UInt`,
            // or an enum.
            Type::Data(data @ (Data::Enum(_) | Data::Number(crate::model::Number::U32))) => {
                format!("({take} : {})", data::haxe_type(data))
            }
            _ => take,
        };
        // A failure with one of the library's error types is thrown as its
        // typed exception.
        let take = if kind != Kind::Signal && !library.errors().is_empty() {
            format!(
                "try {take} catch (error:haxe.Exception) throw {}.typed(error)",
                thrown_class(library)
            )
        } else {
            take
        };
        // A primitive is wrapped in a closure where a function value is
        // needed.
        let parts = match kind {
            Kind::Future => format!(
                "() -> Native.future_watch(hxnRaw), () -> Native.future_state(hxnRaw), limit -> Native.future_wait(hxnRaw, limit), () -> {take}"
            ),
            Kind::Stream => format!(
                "() -> Native.stream_watch(hxnRaw), limit -> Native.stream_wait(hxnRaw, limit), () -> {take}"
            ),
            Kind::Signal => format!(
                "() -> Native.signal_watch(hxnRaw), (seen, limit) -> Native.signal_wait(hxnRaw, seen, limit), () -> {take}"
            ),
        };
        if index != 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "\tpublic static function {}(hxnRaw:{raw}):hxn.{}<{}>\n\t\treturn {};\n",
            maker(carried),
            kind.title(),
            haxe_type(carried),
            haxe_new(library, kind, &parts)
        ));
    }
    out.push_str("}\n");
    File {
        path: format!("{name}.hx"),
        text: out,
    }
}

fn cpp_file(library: &Library, kind: Kind, carried: &[Type]) -> File {
    let name = class(library, kind);
    let local = kind.name();
    let native = native_class(library);
    let mut out = format!(
        "{HEADER}/** Owns a {local} the library returned, and makes the `hxn.{title}` over it. **/\n@:noCompletion class {name} {{\n\t@:unreflective public var hxnRaw:cpp.RawPointer<cpp.Void>;\n\n\t// Runs inside a collection: it may free, and may not touch the Haxe heap.\n\tstatic function hxnFinalize(self:{name}):Void {{\n\t\t{native}.release(self.hxnRaw);\n\t}}\n\n\tpublic static function hxnAdopt():{name}\n\t\treturn Type.createEmptyInstance({name});\n\n\tpublic function hxnOwn():Void\n\t\tcpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(hxnFinalize));\n",
        title = kind.title(),
    );
    for (call, params, _, _) in calls(kind) {
        let names = names(params);
        let declared: Vec<String> = names.iter().map(|n| format!("{n}:Int")).collect();
        let passed: String = names.iter().map(|n| format!(", {n}")).collect();
        out.push_str(&format!(
            "\n\tfunction {call}({}):Int\n\t\treturn {native}.{local}_{call}(hxnRaw{passed});\n",
            declared.join(", ")
        ));
    }
    let parts = match kind {
        Kind::Future => "watch, state, wait",
        _ => "watch, wait",
    };
    for carried in carried {
        let title = title(carried);
        let call = format!(
            "{native}.{local}_{}_{}(hxnRaw)",
            kind.take(),
            suffix(carried)
        );
        match value_type(carried) {
            Type::List { interface } => {
                let stream = class(library, Kind::Stream);
                out.push_str(&format!(
                    "\n\tfunction take{title}():Array<{interface}> {{\n\t\tvar made = {stream}.hxnAdopt();\n\t\tmade.hxnRaw = {call};\n\t\tmade.hxnOwn();\n\t\treturn [for (item in made.of{interface}()) item];\n\t}}\n"
                ));
            }
            Type::Handle {
                interface,
                nullable,
            } => out.push_str(&format!(
                "\n\tfunction take{title}():{} {{\n\t\tvar made = {interface}.hxnAdopt();\n\t\tmade.hxnRaw = {call};\n{}\t\tmade.hxnOwn();\n\t\treturn made;\n\t}}\n",
                haxe_type(carried),
                if *nullable {
                    "\t\tif (made.hxnRaw == null)\n\t\t\treturn null;\n"
                } else {
                    ""
                }
            )),
            Type::Void => out.push_str(&format!(
                "\n\tfunction take{title}():hxn.Unit {{\n\t\t{call};\n\t\treturn hxn.Unit.Nothing;\n\t}}\n"
            )),
            Type::Bytes => out.push_str(&format!(
                "\n\tfunction take{title}():haxe.io.Bytes\n\t\treturn haxe.io.Bytes.ofData({call});\n"
            )),
            Type::Data(data) if crossed(carried) == Type::Bytes => out.push_str(&format!(
                "\n\tfunction take{title}():{} {{\n\t\tvar input = new haxe.io.BytesInput(haxe.io.Bytes.ofData({call}));\n\t\treturn {};\n\t}}\n",
                haxe_type(carried),
                data::haxe_read(data)
            )),
            _ => out.push_str(&format!(
                "\n\tfunction take{title}():{}\n\t\treturn {call};\n",
                haxe_type(carried)
            )),
        }
        // A failure with one of the library's error types is thrown as its
        // typed exception.
        let take = if kind != Kind::Signal && !library.errors().is_empty() {
            format!(
                "() -> try take{title}() catch (error:haxe.Exception) throw {}.typed(error)",
                thrown_class(library)
            )
        } else {
            format!("take{title}")
        };
        out.push_str(&format!(
            "\n\tpublic function {}():hxn.{}<{}>\n\t\treturn {};\n",
            maker(carried),
            kind.title(),
            haxe_type(carried),
            haxe_new(library, kind, &format!("{parts}, {take}"))
        ));
    }
    out.push_str("}\n");
    File {
        path: format!("{name}.hx"),
        text: out,
    }
}

const INBOX_DOCS: &str = "Where everything the library produces over time reaches the app.";

/// The Haxe classes HashLink needs for these: one per kind, and the inbox.
pub fn hl_files(library: &Library) -> Vec<File> {
    let used = used(library);
    if used.is_empty() {
        return Vec::new();
    }
    let mut files: Vec<File> = used
        .iter()
        .map(|(kind, carried)| hl_file(library, *kind, carried))
        .collect();
    let name = inbox_class(library);
    files.push(File {
        path: format!("{name}.hx"),
        text: format!(
            "{HEADER}@:hlNative(\"{}\")\nprivate class Native {{\n\tpublic static function outbox_wait(milliseconds:Int):Bool\n\t\treturn false;\n\n\tpublic static function outbox_next():Int\n\t\treturn -1;\n}}\n\n/** {INBOX_DOCS} **/\nclass {name} {{\n\tpublic static final inbox = new hxn.Inbox(limit -> Native.outbox_wait(limit), () -> Native.outbox_next());\n}}\n",
            library.name
        ),
    });
    files
}

/// The same for hxcpp.
pub fn cpp_files(library: &Library) -> Vec<File> {
    let used = used(library);
    if used.is_empty() {
        return Vec::new();
    }
    let mut files: Vec<File> = used
        .iter()
        .map(|(kind, carried)| cpp_file(library, *kind, carried))
        .collect();
    let name = inbox_class(library);
    let native = native_class(library);
    files.push(File {
        path: format!("{name}.hx"),
        text: format!(
            "{HEADER}/** {INBOX_DOCS} **/\nclass {name} {{\n\tpublic static final inbox = new hxn.Inbox(limit -> {native}.outbox_wait(limit), () -> {native}.outbox_next());\n}}\n"
        ),
    });
    files
}

/// What the library's hxcpp extern class declares for these.
pub fn cpp_natives(library: &Library) -> String {
    let used = used(library);
    if used.is_empty() {
        return String::new();
    }
    let this = "self:Handle";
    let mut out = cpp_extern(library, "outbox_wait", "milliseconds:Int", "Bool");
    out.push_str(&cpp_extern(library, "outbox_next", "", "Int"));
    for (kind, carried) in &used {
        let local = kind.name();
        for (call, params, _, _) in calls(*kind) {
            let declared: String = names(params).iter().map(|n| format!(", {n}:Int")).collect();
            out.push_str(&cpp_extern(
                library,
                &format!("{local}_{call}"),
                &format!("{this}{declared}"),
                "Int",
            ));
        }
        for carried in carried {
            let returns = match &crossed(carried) {
                Type::Handle { .. } | Type::List { .. } => String::from("Handle"),
                Type::Void => String::from("Bool"),
                Type::I64 => String::from("cpp.Int64"),
                Type::Bytes => String::from("haxe.io.BytesData"),
                other => haxe_type(other),
            };
            out.push_str(&cpp_extern(
                library,
                &format!("{local}_{}_{}", kind.take(), suffix(carried)),
                this,
                &returns,
            ));
        }
    }
    out
}
