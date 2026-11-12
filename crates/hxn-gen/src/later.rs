//! Results over time: what a library that returns a `Future`, a `Stream` or
//! a `Signal` needs beside its own functions.
//!
//! Each is handed to the host type-erased, as a handle of `AnyFuture`,
//! `AnyStream` or `AnySignal`. The library then exports, once for all its
//! functions, the calls that wait on one and watch it, one call a kind of
//! value to take what it carries, and its queue of what is ready. On the
//! Haxe side a class a kind turns such a handle into an `hxn.Future`,
//! `hxn.Stream` or `hxn.Signal`.

use crate::haxe::{File, cpp_extern};
use crate::model::{Library, Type};
use crate::names::{abstract_name, native_class, snake};
use crate::rust::held;

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

    /// The call that hands over what it carries.
    fn take(self) -> &'static str {
        match self {
            Self::Future => "take",
            Self::Stream => "next",
            Self::Signal => "get",
        }
    }
}

/// The kind of a result over time, and what it carries.
pub fn of(ty: &Type) -> Option<(Kind, &Type)> {
    match ty {
        Type::Future(carried) => Some((Kind::Future, carried)),
        Type::Stream(carried) => Some((Kind::Stream, carried)),
        Type::Signal(carried) => Some((Kind::Signal, carried)),
        _ => None,
    }
}

/// Each kind the library returns, with everything that kind carries
/// somewhere in it, in the order met.
pub fn used(library: &Library) -> Vec<(Kind, Vec<Type>)> {
    let mut out: Vec<(Kind, Vec<Type>)> = Vec::new();
    for method in library.interfaces.iter().flat_map(|i| &i.methods) {
        let Some((kind, carried)) = of(&method.returns) else {
            continue;
        };
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
    }
    out
}

fn interface(carried: &Type) -> Option<&str> {
    match carried {
        Type::Handle { interface, .. } => Some(interface),
        _ => None,
    }
}

// What a carried type is called in a symbol: `int`, `text`, `counter`.
fn suffix(carried: &Type) -> String {
    match carried {
        Type::Void => String::from("unit"),
        Type::Bool => String::from("bool"),
        Type::I32 => String::from("int"),
        Type::F64 => String::from("float"),
        Type::Str => String::from("text"),
        Type::Handle { interface, .. } => snake(interface),
        _ => unreachable!("not carried by a result over time"),
    }
}

// The same in a Haxe name: `Int`, `Text`, `Counter`.
fn title(carried: &Type) -> String {
    match carried {
        Type::Void => String::from("Unit"),
        Type::Bool => String::from("Bool"),
        Type::I32 => String::from("Int"),
        Type::F64 => String::from("Float"),
        Type::Str => String::from("Text"),
        Type::Handle { interface, .. } => interface.clone(),
        _ => unreachable!("not carried by a result over time"),
    }
}

fn haxe_type(carried: &Type) -> String {
    match carried {
        Type::Void => String::from("hxn.Unit"),
        Type::Bool => String::from("Bool"),
        Type::I32 => String::from("Int"),
        Type::F64 => String::from("Float"),
        Type::Str => String::from("String"),
        Type::Handle { interface, .. } => interface.clone(),
        _ => unreachable!("not carried by a result over time"),
    }
}

// The Rust type a library's own future, stream or signal carries.
fn rust_type(library: &Library, carried: &Type) -> String {
    match carried {
        Type::Void => String::from("()"),
        Type::Bool => String::from("bool"),
        Type::I32 => String::from("i32"),
        Type::F64 => String::from("f64"),
        Type::Str => String::from("String"),
        Type::Handle { interface, .. } => held(library, interface),
        _ => unreachable!("not carried by a result over time"),
    }
}

/// What HashLink knows a kind's handles as: `counter_future`.
pub fn handle_abstract(library: &Library, kind: Kind) -> String {
    format!("{}_{}", library.name, kind.name())
}

// `Hxn<Library><role>`, as the other classes a binding needs are named.
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

/// The Haxe class that makes a kind's objects: `HxnCounterFuture`.
pub fn class(library: &Library, kind: Kind) -> String {
    helper(library, kind.title())
}

/// The Haxe class that holds the library's inbox: `HxnCounterInbox`.
pub fn inbox_class(library: &Library) -> String {
    helper(library, "Inbox")
}

/// The name of the Haxe function that makes one carrying `carried`: `ofInt`.
pub fn maker(carried: &Type) -> String {
    format!("of{}", title(carried))
}

fn export(library: &Library, kind: Kind, call: &str) -> String {
    format!("{}_{}_{call}", library.name, kind.name())
}

/// What an adapter imports for these. Nothing, for a library with none, so
/// that its adapter crate need not depend on `hxn-future`.
pub fn imports(library: &Library) -> &'static str {
    if used(library).is_empty() {
        ""
    } else {
        "#[allow(unused_imports)]\nuse hxn_future::{AnyFuture, AnySignal, AnyStream, outbox};\n"
    }
}

/// A function's result as the handle the host gets, over the C ABI.
pub fn c_result(kind: Kind, call: &str) -> String {
    format!("Ok(Handle::into_raw(Any{}::from({call})))", kind.title())
}

/// The same for HashLink.
pub fn hl_result(kind: Kind, call: &str) -> String {
    format!("Ok(manage(Any{}::from({call})))", kind.title())
}

// The names in `milliseconds: i32, `, each an int.
fn names(params: &str) -> Vec<&str> {
    params
        .split(", ")
        .filter_map(|param| param.split(':').next())
        .filter(|param| !param.is_empty())
        .collect()
}

// The calls every handle of a kind answers, beside taking what it carries:
// name, extra parameters, and the expression over `this`.
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
    let name = &library.name;
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
            // What the erased handle gives is already a `Result`.
            let value = format!("this.{}::<{}>()", kind.take(), rust_type(library, carried));
            let (ty, result) = match carried {
                // Nothing crosses as a flag that is always set: taking it
                // is what says the work is done, or fails.
                Type::Void => ("i32", format!("{value}.map(|()| 1)")),
                Type::Bool => ("i32", format!("Ok(i32::from({value}?))")),
                Type::I32 => ("i32", value),
                Type::F64 => ("f64", value),
                Type::Str => ("Buf", format!("Ok(Buf::from({value}?))")),
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

/// The same as HashLink primitives.
pub fn hl_adapter(library: &Library) -> String {
    let used = used(library);
    if used.is_empty() {
        return String::new();
    }
    let name = &library.name;
    let mut out = format!(
        "\n// The library's one queue of what it has for the host.\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {name}_outbox_wait(milliseconds: i32) -> bool {{\n    unsafe {{ blocking(|| outbox::wait_millis(milliseconds)) }}\n}}\ndefine_prim!(hlp_outbox_wait, {name}_outbox_wait, \"Pi_b\");\n\n#[unsafe(no_mangle)]\npub extern \"C\" fn {name}_outbox_next() -> i32 {{\n    outbox::next()\n}}\ndefine_prim!(hlp_outbox_next, {name}_outbox_next, \"P_i\");\n"
    );
    for (kind, carried) in &used {
        let any = format!("Any{}", kind.title());
        let own = handle_abstract(library, *kind);
        for (call, params, body, waits) in calls(*kind) {
            let letters = "i".repeat(params.matches(": i32").count());
            // The collector may run while a call waits.
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
                "\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {export}(this: *mut Managed{params}) -> i32 {{\n    unsafe {{\n        prim(|| {{\n            let this = handle::<{any}>(this)?.borrow()?;\n            Ok({body})\n        }})\n    }}\n}}\ndefine_prim!(hlp_{local}_{call}, {export}, \"PX{own}_{letters}_i\");\n",
                local = kind.name(),
            ));
        }
        for carried in carried {
            let value = format!("this.{}::<{}>()", kind.take(), rust_type(library, carried));
            let (ty, letter, result) = match carried {
                Type::Void => ("bool", String::from("b"), format!("{value}.map(|()| true)")),
                Type::Bool => ("bool", String::from("b"), value),
                Type::I32 => ("i32", String::from("i"), value),
                Type::F64 => ("f64", String::from("d"), value),
                Type::Str => (
                    "*mut vbyte",
                    String::from("B"),
                    format!("Ok(utf16(&{value}?))"),
                ),
                _ => (
                    "*mut Managed",
                    format!(
                        "X{}_",
                        abstract_name(library, interface(carried).expect("an interface"))
                    ),
                    format!("Ok(manage({value}?))"),
                ),
            };
            let call = format!("{}_{}", kind.take(), suffix(carried));
            let export = export(library, *kind, &call);
            out.push_str(&format!(
                "\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {export}(this: *mut Managed) -> {ty} {{\n    unsafe {{\n        prim(|| {{\n            let this = handle::<{any}>(this)?.borrow()?;\n            {result}\n        }})\n    }}\n}}\ndefine_prim!(hlp_{local}_{call}, {export}, \"PX{own}__{letter}\");\n",
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
    let name = &library.name;
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
            let ty = match carried {
                Type::Void | Type::Bool | Type::I32 => "int32_t *",
                Type::F64 => "double *",
                Type::Str => "hxn_buf *",
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
    let name = &library.name;
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
                // The zone ends before anything is thrown.
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
            let (returns, local, result) = match carried {
                Type::Void | Type::Bool => ("bool ", "int out = 0;", "out != 0"),
                Type::I32 => ("int ", "int out = 0;", "out"),
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

// The Haxe expression that makes the object over a handle, given the
// functions that watch it, wait on it and take from it.
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
        let (ty, nothing) = match carried {
            Type::Void | Type::Bool => (String::from("Bool"), "false"),
            Type::I32 => (String::from("Int"), "0"),
            Type::F64 => (String::from("Float"), "0."),
            Type::Str => (String::from("hl.Bytes"), "null"),
            _ => {
                let interface = interface(carried).expect("an interface");
                (format!("{interface}.{interface}Raw"), "null")
            }
        };
        out.push_str(&format!(
            "\tpublic static function {local}_{}_{}(self:{raw}):{ty}\n\t\treturn {nothing};\n\n",
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
        let take = format!("Native.{local}_{}_{}(raw)", kind.take(), suffix(carried));
        let take = match carried {
            Type::Void => format!("{{\n\t\t\t{take};\n\t\t\thxn.Unit.Nothing;\n\t\t}}"),
            Type::Str => format!("@:privateAccess String.fromUCS2({take})"),
            Type::Handle { interface, .. } => format!("{interface}.adopt({take})"),
            _ => take,
        };
        // A primitive is wrapped where a function value is needed.
        let parts = match kind {
            Kind::Future => format!(
                "() -> Native.future_watch(raw), () -> Native.future_state(raw), limit -> Native.future_wait(raw, limit), () -> {take}"
            ),
            Kind::Stream => format!(
                "() -> Native.stream_watch(raw), limit -> Native.stream_wait(raw, limit), () -> {take}"
            ),
            Kind::Signal => format!(
                "() -> Native.signal_watch(raw), (seen, limit) -> Native.signal_wait(raw, seen, limit), () -> {take}"
            ),
        };
        if index != 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "\tpublic static function {}(raw:{raw}):hxn.{}<{}>\n\t\treturn {};\n",
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
        "{HEADER}/** Owns a {local} the library returned, and makes the `hxn.{title}` over it. **/\n@:noCompletion class {name} {{\n\t@:unreflective public var raw:cpp.RawPointer<cpp.Void>;\n\n\t// Runs inside a collection: it may free, and may not touch the Haxe heap.\n\tstatic function finalize(self:{name}):Void {{\n\t\t{native}.release(self.raw);\n\t}}\n\n\tpublic static function adopt():{name}\n\t\treturn Type.createEmptyInstance({name});\n\n\tpublic function own():Void\n\t\tcpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));\n",
        title = kind.title(),
    );
    for (call, params, _, _) in calls(kind) {
        let names = names(params);
        let declared: Vec<String> = names.iter().map(|n| format!("{n}:Int")).collect();
        let passed: String = names.iter().map(|n| format!(", {n}")).collect();
        out.push_str(&format!(
            "\n\tfunction {call}({}):Int\n\t\treturn {native}.{local}_{call}(raw{passed});\n",
            declared.join(", ")
        ));
    }
    let parts = match kind {
        Kind::Future => "watch, state, wait",
        _ => "watch, wait",
    };
    for carried in carried {
        let title = title(carried);
        let call = format!("{native}.{local}_{}_{}(raw)", kind.take(), suffix(carried));
        match carried {
            Type::Handle { interface, .. } => out.push_str(&format!(
                "\n\tfunction take{title}():{interface} {{\n\t\tvar made = {interface}.adopt();\n\t\tmade.raw = {call};\n\t\tmade.own();\n\t\treturn made;\n\t}}\n"
            )),
            Type::Void => out.push_str(&format!(
                "\n\tfunction take{title}():hxn.Unit {{\n\t\t{call};\n\t\treturn hxn.Unit.Nothing;\n\t}}\n"
            )),
            _ => out.push_str(&format!(
                "\n\tfunction take{title}():{}\n\t\treturn {call};\n",
                haxe_type(carried)
            )),
        }
        out.push_str(&format!(
            "\n\tpublic function {}():hxn.{}<{}>\n\t\treturn {};\n",
            maker(carried),
            kind.title(),
            haxe_type(carried),
            haxe_new(library, kind, &format!("{parts}, take{title}"))
        ));
    }
    out.push_str("}\n");
    File {
        path: format!("{name}.hx"),
        text: out,
    }
}

const INBOX_DOCS: &str = "Where everything the library produces over time reaches the app.";

/// The classes HashLink needs for these: one a kind, and the inbox.
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
            let returns = match carried {
                Type::Handle { .. } => String::from("Handle"),
                Type::Void => String::from("Bool"),
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
