//! The Rust bindings for an [`Api`]: `extern "C"` declarations, and a safe
//! API over them marked with hxn's attributes.
//!
//! - An object is a struct that owns its C pointer and frees it in `Drop`.
//!   Its methods take `&mut self`, so at most one call runs on an object at
//!   a time: a C library is not assumed to be thread-safe. It is `Send`, so
//!   the garbage collector can free it from its own thread.
//! - A function that can fail returns `Result<_, String>`. It can fail when
//!   it takes text (which may contain a NUL), converts a platform-sized
//!   integer, returns an enum (the C value may name no variant) or an object
//!   (the pointer may be null), or has `[HxnStatus]`.
//! - Returned text is copied. Invalid UTF-8 is replaced, not refused.
//! - A callback C keeps, `[HxnKept]`, is a trampoline that copies what C
//!   calls it with and sends it to a stream, which the function returns.
//!   The stream's sender is owned by the object the function was called on
//!   and dropped after the object's C free function runs, which ends the
//!   stream. A free or static function, or a borrowed object, never frees
//!   it.

use std::collections::BTreeSet;

use crate::idl::namespace;
use crate::model::{Api, Callback, Function, Ignored, Int, Object, Param, Role, Status, Ty, freed};

// Rust's keywords, reserved words among them.
const KEYWORDS: [&str; 52] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "union", "unsafe", "unsized", "use", "virtual", "where", "while",
];

// Methods every hxn class has already.
const METHODS: [&str; 3] = ["close", "same", "weak"];

// A name usable as a method's.
fn method_name(name: &str) -> String {
    if METHODS.contains(&name) {
        format!("{name}_c")
    } else {
        ident(name)
    }
}

// Names the adapters and this generator use for their own variables.
const TAKEN: [&str; 5] = ["this", "out", "err", "raw", "status"];

// A name usable as a Rust identifier and as an hxn parameter.
fn ident(name: &str) -> String {
    if KEYWORDS.contains(&name) || TAKEN.contains(&name) {
        format!("{name}_value")
    } else {
        name.to_owned()
    }
}

// What the C function takes or returns for `ty`.
fn c_type(ty: &Ty) -> String {
    match ty {
        Ty::Void => String::from("()"),
        Ty::Bool => String::from("bool"),
        Ty::Int(int) => String::from(match int {
            Int::I8 => "i8",
            Int::U8 => "u8",
            Int::I16 => "i16",
            Int::U16 => "u16",
            Int::I32 => "i32",
            Int::U32 => "u32",
            Int::I64 => "i64",
            Int::U64 => "u64",
            Int::Long => "c_long",
            Int::ULong => "c_ulong",
            Int::Size => "usize",
            Int::SSize => "isize",
        }),
        Ty::F32 => String::from("f32"),
        Ty::F64 => String::from("f64"),
        Ty::Text | Ty::Nullable(_) if matches!(ty.inner(), Ty::Text) => {
            String::from("*const c_char")
        }
        Ty::Bytes(_) => String::from("*const u8"),
        Ty::Enum(_) => String::from("c_int"),
        Ty::Struct(name) => format!("ffi::{name}"),
        Ty::StructRef(name) => format!("*const ffi::{name}"),
        Ty::Object(name) => format!("*mut ffi::{name}"),
        Ty::Nullable(of) => c_type(of),
        Ty::Callback(_) | Ty::Pointer => String::from("*mut c_void"),
        Ty::Text => unreachable!(),
    }
}

impl Ty {
    fn inner(&self) -> &Ty {
        match self {
            Self::Nullable(of) => of,
            other => other,
        }
    }
}

// What the safe function takes for `ty`.
fn rust_param(ty: &Ty) -> String {
    match ty {
        Ty::Int(Int::Long | Int::SSize) => String::from("i64"),
        Ty::Int(Int::ULong | Int::Size) => String::from("u64"),
        Ty::Text => String::from("&str"),
        Ty::Bytes(_) => String::from("&[u8]"),
        Ty::Enum(name) | Ty::Struct(name) => name.clone(),
        Ty::StructRef(name) => format!("&{name}"),
        Ty::Object(name) => format!("&mut {name}"),
        Ty::Nullable(of) => format!("Option<{}>", rust_param(of)),
        other => c_type(other),
    }
}

// What the safe function returns for `ty`, less any `Result`.
fn rust_return(ty: &Ty) -> String {
    match ty {
        Ty::Void => String::from("()"),
        Ty::Text => String::from("String"),
        Ty::Nullable(of) => format!("Option<{}>", rust_return(of)),
        Ty::Object(name) | Ty::Enum(name) | Ty::Struct(name) => name.clone(),
        other => rust_param(other),
    }
}

fn fallible_param(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Text | Ty::Int(Int::Long | Int::ULong | Int::Size | Int::SSize)
    ) || matches!(ty, Ty::Bytes(length) if *length != Int::Size)
}

fn fallible(function: &Function, constructor: bool) -> bool {
    constructor
        || lends(function)
        || function.status != Status::None
        || function
            .params
            .iter()
            .any(|param| param.role.passed() && fallible_param(&param.ty))
        || matches!(function.returns, Ty::Enum(_) | Ty::Object(_))
}

// Whether the function takes a Haxe function, which makes its error the
// one that function fails with.
fn lends(function: &Function) -> bool {
    function
        .params
        .iter()
        .any(|param| param.role == Role::In && matches!(param.ty, Ty::Callback(_)))
}

// Every function of `api`, with the object it belongs to.
fn all_functions(api: &Api) -> impl Iterator<Item = &Function> {
    api.objects
        .iter()
        .flat_map(|object| {
            object
                .constructor
                .iter()
                .chain(&object.methods)
                .chain(&object.statics)
        })
        .chain(&api.functions)
}

// The callbacks some function lends, or keeps when `role` is `Kept`.
fn used(api: &Api, role: Role) -> Vec<&Callback> {
    api.callbacks
        .iter()
        .filter(|known| {
            all_functions(api).any(|function| {
                function
                    .params
                    .iter()
                    .any(|p| p.role == role && p.ty == Ty::Callback(known.name.clone()))
            })
        })
        .collect()
}

// Whether `object` owns the senders of the callbacks its methods keep: it
// has a free function, after which C calls none of them.
fn keeps(object: &Object) -> bool {
    object.free.is_some() && object.methods.iter().any(|m| m.kept().is_some())
}

// The object `name` made from a pointer.
fn made(api: &Api, name: &str, raw: &str, owned: bool) -> String {
    let kept = api
        .objects
        .iter()
        .any(|object| object.name == name && keeps(object));
    if kept {
        format!("{name} {{ raw: {raw}, owned: {owned}, kept: Default::default() }}")
    } else {
        format!("{name} {{ raw: {raw}, owned: {owned} }}")
    }
}

// What a stream of a kept callback's calls carries, for a value of `ty`,
// and the expression that makes it from `name`, the C value.
fn kept_value(ty: &Ty, name: &str) -> (String, String) {
    let (carried, value) = match ty {
        Ty::Bool => ("bool", name.to_owned()),
        Ty::Int(Int::I8 | Int::U8 | Int::I16 | Int::U16 | Int::I32) => {
            ("i32", format!("i32::from({name})"))
        }
        Ty::Int(Int::U32 | Int::I64 | Int::Long) => ("i64", format!("i64::from({name})")),
        Ty::Int(Int::SSize) => ("i64", format!("{name} as i64")),
        Ty::Int(Int::U64) => ("u64", name.to_owned()),
        Ty::Int(Int::ULong) => ("u64", format!("u64::from({name})")),
        Ty::Int(Int::Size) => ("u64", format!("{name} as u64")),
        Ty::F32 => ("f64", format!("f64::from({name})")),
        Ty::F64 => ("f64", name.to_owned()),
        Ty::Text => (
            "String",
            format!("unsafe {{ hxn_copy({name}) }}.unwrap_or_default()"),
        ),
        // Converted ahead of the others, since it can fail.
        Ty::Enum(known) => return (known.clone(), name.to_owned()),
        Ty::Struct(record) => return (record.clone(), format!("{record}::from({name})")),
        other => unreachable!("a callback does not take {other:?}"),
    };
    (carried.to_owned(), value)
}

// Whether the stream of `callback`'s calls carries one record per call,
// named `<Callback>Event`, rather than the value it is called with.
fn has_event(callback: &Callback) -> bool {
    match callback.params.as_slice() {
        [] => false,
        [one] => kept_value(&one.ty, "").0 == "u64",
        _ => true,
    }
}

// What the stream of `callback`'s calls carries.
fn event_type(callback: &Callback) -> String {
    match callback.params.as_slice() {
        [] => String::from("()"),
        _ if has_event(callback) => format!("{}Event", callback.name),
        [one] => kept_value(&one.ty, "").0,
        _ => unreachable!(),
    }
}

// The record of one call of `callback`, and the trampoline C calls for it
// when it keeps it. The trampoline sends what C passed to the stream whose
// sender is the user data, and returns `[HxnStop]` to C.
fn keeper(callback: &Callback) -> String {
    let symbol = callback_symbol(&callback.name);
    let carried = event_type(callback);
    let mut out = String::new();
    if has_event(callback) {
        let fields: String = callback
            .params
            .iter()
            .map(|p| {
                format!(
                    "    pub {}: {},
",
                    ident(&p.name),
                    kept_value(&p.ty, "").0
                )
            })
            .collect();
        out.push_str(&format!(
            "\n/// What C called a `{name}` with.\n#[hxn::data]\n#[derive(Clone, Debug, PartialEq)]\npub struct {carried} {{\n{fields}}}\n",
            name = callback.name
        ));
    }
    let mut params: Vec<String> = callback
        .params
        .iter()
        .map(|p| format!("{}: {}", ident(&p.name), c_type(&p.ty)))
        .collect();
    if callback.user_first {
        params.insert(0, String::from("user: *mut c_void"));
    } else {
        params.push(String::from("user: *mut c_void"));
    }
    let stop = callback.stop.unwrap_or(0);
    let (returns, answer) = match callback.returns {
        Ty::Void => (String::new(), String::new()),
        Ty::Bool => (String::from(" -> bool"), format!("\n    {}", stop != 0)),
        _ => (String::from(" -> c_int"), format!("\n    {stop}")),
    };
    let mut converted = Vec::new();
    let mut values = Vec::new();
    for param in &callback.params {
        let name = ident(&param.name);
        if let Ty::Enum(known) = &param.ty {
            converted.push(format!(
                "let {name} = match {known}::from_c({name}) {{\n            Ok(value) => value,\n            Err(error) => {{\n                hxn_sender.fail(error);\n                return;\n            }}\n        }};\n        "
            ));
        }
        values.push((name.clone(), kept_value(&param.ty, &name).1));
    }
    let item = match callback.params.as_slice() {
        [] => String::from("()"),
        _ if has_event(callback) => format!(
            "{carried} {{\n            {}\n        }}",
            values
                .iter()
                .map(|(name, value)| if name == value {
                    format!("{name},")
                } else {
                    format!("{name}: {value},")
                })
                .collect::<Vec<_>>()
                .join("\n            ")
        ),
        _ => values[0].1.clone(),
    };
    out.push_str(&format!(
        "\n// Called by C for a `{name}` it keeps, with the sender of the stream in\n// `user`.\nunsafe extern \"C\" fn hxn_{symbol}_keep({params}){returns} {{\n    let hxn_sender = unsafe {{ &*user.cast::<hxn_future::Sender<{carried}>>() }};\n    // C cannot be unwound through.\n    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{\n        {converted}hxn_sender.send({item});\n    }}));{answer}\n}}\n",
        name = callback.name,
        params = params.join(", "),
        converted = converted.concat(),
    ));
    out
}

const FREES: &str = "\n// Text C is given to keep, which it frees with `hxn_free_text`.\nunsafe extern \"C\" fn hxn_free_text(text: *mut c_void) {\n    drop(unsafe { CString::from_raw(text.cast()) });\n}\n\n// The size of the length kept in front of bytes C is given.\nconst HXN_HEADER: usize = size_of::<usize>();\n\n// A copy of `bytes` C is given to keep, which it frees with `hxn_free_bytes`.\n// The copy's length is kept in front of it.\nfn hxn_owned_bytes(bytes: &[u8]) -> *const u8 {\n    let mut owned = Vec::with_capacity(HXN_HEADER + bytes.len());\n    owned.extend_from_slice(&bytes.len().to_ne_bytes());\n    owned.extend_from_slice(bytes);\n    let start = Box::into_raw(owned.into_boxed_slice()).cast::<u8>();\n    unsafe { start.add(HXN_HEADER) }\n}\n\n// Frees bytes `hxn_owned_bytes` made.\nunsafe extern \"C\" fn hxn_free_bytes(bytes: *mut c_void) {\n    let start = unsafe { bytes.cast::<u8>().sub(HXN_HEADER) };\n    let length = usize::from_ne_bytes(unsafe { start.cast::<[u8; HXN_HEADER]>().read() });\n    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(start, HXN_HEADER + length)) });\n}\n";

const KEPT: &str = "\n// The sender of a stream, which C holds a pointer to as a callback's user\n// data. Dropping it frees the sender and ends the stream, so it is dropped\n// only once C can no longer call the callback.\nstruct Kept<T>(*mut hxn_future::Sender<T>);\n\n// The sender is `Send`, and C only reads it.\nunsafe impl<T: Send> Send for Kept<T> {}\n\nimpl<T> Kept<T> {\n    fn new(sender: hxn_future::Sender<T>) -> Self {\n        Self(Box::into_raw(Box::new(sender)))\n    }\n\n    // What C is passed as the user data.\n    fn user(&self) -> *mut c_void {\n        self.0.cast()\n    }\n}\n\nimpl<T> Drop for Kept<T> {\n    fn drop(&mut self) {\n        drop(unsafe { Box::from_raw(self.0) });\n    }\n}\n";

fn callback<'a>(api: &'a Api, name: &str) -> &'a Callback {
    api.callbacks
        .iter()
        .find(|known| known.name == name)
        .expect("a callback the WebIDL declares")
}

// The callback's name in the functions generated for it: `visitor`.
fn callback_symbol(name: &str) -> String {
    let mut out = String::new();
    for (at, letter) in name.chars().enumerate() {
        if letter.is_ascii_uppercase() {
            if at > 0 {
                out.push('_');
            }
            out.push(letter.to_ascii_lowercase());
        } else {
            out.push(letter);
        }
    }
    out
}

// The function pointer type C takes for `callback`.
fn callback_pointer(callback: &Callback) -> String {
    let mut params: Vec<String> = callback.params.iter().map(|p| c_type(&p.ty)).collect();
    if callback.user_first {
        params.insert(0, String::from("*mut c_void"));
    } else {
        params.push(String::from("*mut c_void"));
    }
    let returns = match callback.returns {
        Ty::Void => String::new(),
        Ty::Bool => String::from(" -> bool"),
        _ => String::from(" -> c_int"),
    };
    format!("unsafe extern \"C\" fn({}){returns}", params.join(", "))
}

// What the Haxe function takes for a callback parameter of type `ty`.
fn callback_value(ty: &Ty) -> String {
    match ty {
        Ty::F32 => String::from("f64"),
        Ty::Text => String::from("&str"),
        other => rust_return(other),
    }
}

// `impl FnMut(i32) -> Result<i32, E>`, the Haxe function lent for `callback`.
fn callback_bound(callback: &Callback) -> String {
    let params: Vec<String> = callback
        .params
        .iter()
        .map(|p| callback_value(&p.ty))
        .collect();
    format!(
        "FnMut({}) -> Result<{}, E>",
        params.join(", "),
        rust_return(&callback.returns)
    )
}

// The trampoline C calls for `callback`, which calls the Haxe function in
// the user data. A failure or a panic is kept for after C returns, and C is
// told to stop, or not called back again where it cannot be told.
fn trampoline(callback: &Callback) -> String {
    let symbol = callback_symbol(&callback.name);
    let bound = callback_bound(callback);
    let mut params: Vec<String> = callback
        .params
        .iter()
        .map(|p| format!("{}: {}", ident(&p.name), c_type(&p.ty)))
        .collect();
    if callback.user_first {
        params.insert(0, String::from("user: *mut c_void"));
    } else {
        params.push(String::from("user: *mut c_void"));
    }
    let (returns, stop) = match callback.returns {
        Ty::Void => (String::new(), String::new()),
        Ty::Bool => (
            String::from(" -> bool"),
            format!(" {}", callback.stop.unwrap_or(0) != 0),
        ),
        _ => (
            String::from(" -> c_int"),
            format!(" {}", callback.stop.unwrap_or(1)),
        ),
    };
    let mut converted = Vec::new();
    let mut passed = Vec::new();
    for param in &callback.params {
        let name = ident(&param.name);
        match &param.ty {
            Ty::F32 => passed.push(format!("f64::from({name})")),
            Ty::Text => {
                converted.push(format!(
                    "let {name} = if {name}.is_null() {{\n        std::borrow::Cow::Borrowed(\"\")\n    }} else {{\n        unsafe {{ CStr::from_ptr({name}) }}.to_string_lossy()\n    }};"
                ));
                passed.push(format!("&{name}"));
            }
            Ty::Enum(known) => {
                converted.push(format!(
                    "let {name} = match {known}::from_c({name}) {{\n        Ok(value) => value,\n        Err(error) => {{\n            lent.failed = Some(E::from(error));\n            return{stop};\n        }}\n    }};"
                ));
                passed.push(name);
            }
            Ty::Struct(record) => passed.push(format!("{record}::from({name})")),
            _ => passed.push(name),
        }
    }
    let (ok, value) = match callback.returns {
        Ty::Void => ("()", String::from("()")),
        _ => ("value", String::from("value")),
    };
    format!(
        "\n// Called by C for a `{name}`, with the Haxe function in `user`.\nunsafe extern \"C\" fn hxn_{symbol}_call<F: {bound}, E: From<String>>({params}){returns} {{\n    let lent = unsafe {{ &mut *user.cast::<Lent<F, E>>() }};\n    if lent.failed.is_some() || lent.panic.is_some() {{\n        return{stop};\n    }}\n    {converted}match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (lent.f)({passed}))) {{\n        Ok(Ok({ok})) => {value},\n        Ok(Err(error)) => {{\n            lent.failed = Some(error);\n            {stop_value}\n        }}\n        Err(panic) => {{\n            lent.panic = Some(panic);\n            {stop_value}\n        }}\n    }}\n}}\n\n// The trampoline for the Haxe function `lent` holds, whose type has no name.\nfn hxn_{symbol}_for<F: {bound}, E: From<String>>(_: &Lent<F, E>) -> {pointer} {{\n    hxn_{symbol}_call::<F, E>\n}}\n",
        name = callback.name,
        params = params.join(", "),
        converted = converted
            .iter()
            .map(|line| format!("{line}\n    "))
            .collect::<String>(),
        passed = passed.join(", "),
        stop_value = if stop.is_empty() {
            String::from("()")
        } else {
            stop.trim().to_owned()
        },
        pointer = callback_pointer(callback),
    )
}

const LENT: &str = "\n// A Haxe function lent to C for one call, and what went wrong in it. C\n// cannot be unwound through, so a failure or a panic is kept here and raised\n// once C has returned.\nstruct Lent<F, E> {\n    f: F,\n    failed: Option<E>,\n    panic: Option<Box<dyn std::any::Any + Send>>,\n}\n\nimpl<F, E> Lent<F, E> {\n    fn new(f: F) -> Self {\n        Self {\n            f,\n            failed: None,\n            panic: None,\n        }\n    }\n\n    // What C is passed as the user data.\n    fn user(&mut self) -> *mut c_void {\n        (self as *mut Self).cast()\n    }\n\n    // The Haxe function's failure, or its panic resumed.\n    fn finish(self) -> Result<(), E> {\n        if let Some(panic) = self.panic {\n            std::panic::resume_unwind(panic);\n        }\n        self.failed.map_or(Ok(()), Err)\n    }\n}\n";

fn docs(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                format!("{indent}///\n")
            } else {
                format!("{indent}/// {line}\n")
            }
        })
        .collect()
}

// The statements that make each argument what C takes, and the arguments.
fn convert(api: &Api, params: &[Param]) -> (Vec<String>, Vec<String>) {
    let mut before = Vec::new();
    let mut passed = Vec::new();
    // The text and bytes arguments C is given a copy of, to free.
    let owned: Vec<usize> = (0..params.len())
        .filter(|at| params[*at].role == Role::Frees)
        .filter_map(|at| freed(params, at))
        .collect();
    for (at, param) in params.iter().enumerate() {
        let name = ident(&param.name);
        let owned = owned.contains(&at);
        match param.role {
            Role::In | Role::Written => {}
            Role::Frees => {
                let freed = &params[freed(params, at).expect("checked when parsed")];
                passed.push(String::from(if freed.ty == Ty::Text {
                    "Some(hxn_free_text)"
                } else {
                    "Some(hxn_free_bytes)"
                }));
                continue;
            }
            Role::Kept => {
                let Ty::Callback(known) = &param.ty else {
                    unreachable!("only a callback is kept")
                };
                let callback = callback(api, known);
                before.push(format!(
                    "let (hxn_sender, hxn_stream) = hxn_future::channel::<{}>();",
                    event_type(callback)
                ));
                before.push(String::from("let hxn_kept = Kept::new(hxn_sender);"));
                passed.push(format!(
                    "Some(hxn_{}_keep), hxn_kept.user()",
                    callback_symbol(known)
                ));
                continue;
            }
            Role::Null | Role::Pass(0) => {
                passed.push(String::from("std::ptr::null_mut()"));
                continue;
            }
            Role::Pass(value) => {
                passed.push(format!(
                    "std::ptr::without_provenance_mut({value}_isize as usize)"
                ));
                continue;
            }
            // A place for C to write into, read after the call.
            Role::Out => {
                let (init, pointer) = match param.ty.inner() {
                    Ty::Object(_) => (String::from("null_mut()"), format!("&raw mut {name}")),
                    Ty::Text => (String::from("std::ptr::null()"), format!("&raw mut {name}")),
                    Ty::Struct(record) => (
                        format!("std::mem::MaybeUninit::<ffi::{record}>::uninit()"),
                        format!("{name}.as_mut_ptr()"),
                    ),
                    other => (
                        format!("{}::default()", c_type(other)),
                        format!("&raw mut {name}"),
                    ),
                };
                before.push(format!("let mut {name} = {init};"));
                passed.push(pointer);
                continue;
            }
        }
        match &param.ty {
            Ty::Int(int @ (Int::Long | Int::ULong | Int::Size | Int::SSize)) => {
                before.push(format!(
                    "let {name} = {}::try_from({name}).map_err(|_| format!(\"{name}: {{}} does not fit a C {}\", {name}))?;",
                    c_type(&param.ty),
                    int.alias().unwrap_or_default().replace('_', " ")
                ));
                passed.push(name);
            }
            Ty::Text => {
                before.push(format!("let {name} = hxn_text({name})?;"));
                passed.push(if owned {
                    format!("{name}.into_raw()")
                } else {
                    format!("{name}.as_ptr()")
                });
            }
            Ty::Bytes(Int::Size) if param.role == Role::Written => {
                passed.push(format!("{name}.as_mut_ptr(), {name}.len()"));
            }
            Ty::Bytes(Int::Size) if owned => {
                passed.push(format!("hxn_owned_bytes({name}), {name}.len()"));
            }
            Ty::Bytes(Int::Size) => passed.push(format!("{name}.as_ptr(), {name}.len()")),
            Ty::Bytes(length) => {
                before.push(format!(
                    "let {name}_length = {}::try_from({name}.len()).map_err(|_| format!(\"{name}: {{}} bytes do not fit a C {}\", {name}.len()))?;",
                    c_type(&Ty::Int(*length)),
                    length.c_name().replace('_', " ")
                ));
                passed.push(if param.role == Role::Written {
                    format!("{name}.as_mut_ptr(), {name}_length")
                } else if owned {
                    format!("hxn_owned_bytes({name}), {name}_length")
                } else {
                    format!("{name}.as_ptr(), {name}_length")
                });
            }
            Ty::Enum(_) => passed.push(format!("{name} as c_int")),
            Ty::Struct(record) => passed.push(format!("ffi::{record}::from(&{name})")),
            Ty::StructRef(record) => {
                before.push(format!("let {name} = ffi::{record}::from({name});"));
                passed.push(format!("&raw const {name}"));
            }
            Ty::Callback(_) => passed.push(format!("Some({name}_call), {name}.user()")),
            Ty::Object(_) => passed.push(format!("{name}.raw")),
            Ty::Nullable(of) if matches!(**of, Ty::Object(_)) => {
                passed.push(format!("{name}.map_or(null_mut(), |value| value.raw)"));
            }
            _ => passed.push(name),
        }
    }
    (before, passed)
}

// Fails the call when `status`, the C function's return value, reports a
// failure.
fn status_check(function: &Function, status: &str) -> Option<String> {
    let c = &function.c;
    let failed = match function.status {
        Status::None => return None,
        Status::Zero => format!("{status} != 0"),
        Status::Negative => format!("{status} < 0"),
    };
    Some(format!(
        "if {failed} {{\n            return Err(format!(\"{c} failed with status {{{status}}}\"));\n        }}"
    ))
}

// The statements that make `value`, what C gave, the safe function's
// result, ending with the result.
fn convert_return(api: &Api, function: &Function, value: &str) -> Vec<String> {
    let c = &function.c;
    let mut lines = Vec::new();
    let result = match &function.returns {
        Ty::Void => String::from("()"),
        Ty::Int(Int::Long) => format!("i64::from({value})"),
        Ty::Int(Int::ULong) => format!("u64::from({value})"),
        Ty::Int(Int::Size) => format!("{value} as u64"),
        Ty::Int(Int::SSize) => format!("{value} as i64"),
        Ty::Enum(name) => format!("{name}::from_c({value})?"),
        Ty::Struct(name) => format!("{name}::from({value})"),
        Ty::Object(name) => {
            lines.push(format!(
                "if {value}.is_null() {{\n            return Err(String::from(\"{c} returned null\"));\n        }}"
            ));
            made(api, name, value, !function.borrowed)
        }
        Ty::Nullable(of) if matches!(**of, Ty::Object(_)) => {
            let Ty::Object(name) = &**of else {
                unreachable!()
            };
            format!(
                "(!{value}.is_null()).then(|| {})",
                made(api, name, value, !function.borrowed)
            )
        }
        Ty::Text | Ty::Nullable(_) => {
            lines.push(format!("let copied = unsafe {{ hxn_copy({value}) }};"));
            if let Some(free) = &function.free {
                lines.push(format!(
                    "if !{value}.is_null() {{\n            unsafe {{ ffi::{free}({value}.cast_mut()) }};\n        }}"
                ));
            }
            if function.returns == Ty::Text {
                String::from("copied.unwrap_or_default()")
            } else {
                String::from("copied")
            }
        }
        _ => value.to_owned(),
    };
    if fallible(function, false) {
        lines.push(format!("Ok({result})"));
    } else {
        lines.push(result);
    }
    lines
}

// How a method takes its object: `&mut self`, or `&self` for one marked
// thread-safe.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiver {
    None,
    Exclusive,
    Shared,
}

// A safe function over `function`, with `marker` the attribute a free
// function carries. `keeper` is the object of a method whose object owns
// the senders of the callbacks it keeps.
fn wrapper(
    api: &Api,
    function: &Function,
    receiver: Receiver,
    keeper: bool,
    marker: &str,
) -> String {
    let (before, passed) = convert(api, &function.params);
    let mut params: Vec<String> = Vec::new();
    if receiver != Receiver::None {
        params.push(String::from(if receiver == Receiver::Shared {
            "&self"
        } else {
            "&mut self"
        }));
    }
    for param in function.params.iter().filter(|p| p.role.passed()) {
        let ty = match &param.ty {
            Ty::Callback(name) => format!("impl {}", callback_bound(callback(api, name))),
            Ty::Bytes(_) if param.role == Role::Written => String::from("&mut [u8]"),
            other => rust_param(other),
        };
        params.push(format!("{}: {ty}", ident(&param.name)));
    }
    let mut arguments = Vec::new();
    if receiver != Receiver::None {
        arguments.push(String::from("self.raw"));
    }
    arguments.extend(passed);
    let out = function.out();
    let kept = function.kept().map(|param| {
        let Ty::Callback(known) = &param.ty else {
            unreachable!("only a callback is kept")
        };
        callback(api, known)
    });
    let returns = if let Some(kept) = kept {
        format!("Stream<{}>", event_type(kept))
    } else if function.status == Status::Zero && out.is_none() {
        String::from("()")
    } else {
        rust_return(&function.returns)
    };
    let signature = if fallible(function, false) {
        format!(" -> Result<{returns}, String>")
    } else if returns == "()" {
        String::new()
    } else {
        format!(" -> {returns}")
    };
    let mut body: Vec<String> = before;
    let call = format!("unsafe {{ ffi::{}({}) }}", function.c, arguments.join(", "));
    match out {
        None if kept.is_some() => {
            if function.status == Status::None {
                body.push(format!("{call};"));
            } else {
                body.push(format!("let status = {call};"));
                body.extend(status_check(function, "status"));
            }
            if keeper {
                body.push(String::from(
                    "if self.owned {\n            self.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(Box::new(hxn_kept));\n        } else {\n            // Freed by whatever owns the object, which this cannot see.\n            std::mem::forget(hxn_kept);\n        }",
                ));
            } else {
                body.push(String::from(
                    "// Nothing frees what C calls it with, so it is never freed.\n        std::mem::forget(hxn_kept);",
                ));
            }
            body.push(if fallible(function, false) {
                String::from("Ok(hxn_stream)")
            } else {
                String::from("hxn_stream")
            });
        }
        Some(out) => {
            let name = ident(&out.name);
            if function.status == Status::None {
                body.push(format!("{call};"));
            } else {
                body.push(format!("let status = {call};"));
            }
            // An object C wrote is owned before the status is checked, so
            // that it is freed when the call failed.
            let mut value = name.clone();
            match out.ty.inner() {
                Ty::Object(object) => {
                    body.push(format!(
                        "let {name} = (!{name}.is_null()).then(|| {});",
                        made(api, object, &name, true)
                    ));
                }
                Ty::Struct(_) => {}
                _ => {}
            }
            body.extend(status_check(function, "status"));
            match (out.ty.inner(), &function.returns) {
                (Ty::Object(_), Ty::Nullable(_)) => body.push(if fallible(function, false) {
                    format!("Ok({name})")
                } else {
                    name.clone()
                }),
                (Ty::Object(_), _) => body.push(format!(
                    "{name}.ok_or_else(|| String::from(\"{} returned null\"))",
                    function.c
                )),
                (Ty::Struct(_), _) => {
                    // Written by C, which reported success.
                    body.push(format!("let {name} = unsafe {{ {name}.assume_init() }};"));
                    value = name.clone();
                    body.extend(convert_return(api, function, &value));
                }
                _ => body.extend(convert_return(api, function, &value)),
            }
        }
        None if function.returns == Ty::Void && function.status == Status::None => {
            body.push(format!("{call};"));
            if fallible(function, false) {
                body.push(String::from("Ok(())"));
            }
        }
        None if function.status == Status::Zero => {
            body.push(format!("let status = {call};"));
            body.extend(status_check(function, "status"));
            body.push(String::from("Ok(())"));
        }
        None => {
            body.push(format!("let value = {call};"));
            body.extend(status_check(function, "value"));
            body.extend(convert_return(api, function, "value"));
        }
    }
    // A function that lends Haxe functions runs inside a closure, so that
    // what a Haxe function failed with is returned ahead of what the call
    // made of it.
    let (generics, signature, body) = if lends(function) {
        let lent: Vec<&Param> = function
            .params
            .iter()
            .filter(|p| matches!(p.ty, Ty::Callback(_)))
            .collect();
        let mut wrapped: Vec<String> = Vec::new();
        for param in &lent {
            let Ty::Callback(name) = &param.ty else {
                unreachable!()
            };
            let local = ident(&param.name);
            wrapped.push(format!("let mut {local} = Lent::new({local});"));
            wrapped.push(format!(
                "let {local}_call = hxn_{}_for(&{local});",
                callback_symbol(name)
            ));
        }
        wrapped.push(format!(
            "let result: Result<{returns}, String> = (|| {{\n            {}\n        }})();",
            body.join("\n            ")
        ));
        for param in &lent {
            wrapped.push(format!("{}.finish()?;", ident(&param.name)));
        }
        wrapped.push(String::from("result.map_err(E::from)"));
        (
            String::from("<E: From<String>>"),
            format!(" -> Result<{returns}, E>"),
            wrapped,
        )
    } else {
        (String::new(), signature, body)
    };
    format!(
        "{}{marker}    pub fn {}{generics}({}){signature} {{\n        {}\n    }}\n",
        docs(&function.docs, "    "),
        method_name(&function.name),
        params.join(", "),
        body.join("\n        ")
    )
}

// The C parameters `param` is passed as.
fn c_params(api: &Api, param: &Param) -> Vec<String> {
    let name = ident(&param.name);
    if let Ty::Callback(known) = &param.ty {
        return vec![
            format!("{name}: Option<{}>", callback_pointer(callback(api, known))),
            format!("{name}_user: *mut c_void"),
        ];
    }
    match param.role {
        Role::Pass(_) => return vec![format!("{name}: *mut c_void")],
        Role::Frees => {
            return vec![format!(
                "{name}: Option<unsafe extern \"C\" fn(*mut c_void)>"
            )];
        }
        Role::Written => {
            let Ty::Bytes(length) = &param.ty else {
                unreachable!("only bytes are written")
            };
            return vec![
                format!("{name}: *mut u8"),
                format!("{name}_length: {}", c_type(&Ty::Int(*length))),
            ];
        }
        _ => {}
    }
    if param.role != Role::In {
        return vec![format!("{name}: *mut {}", c_type(param.ty.inner()))];
    }
    match &param.ty {
        Ty::Bytes(length) => vec![
            format!("{name}: *const u8"),
            format!("{name}_length: {}", c_type(&Ty::Int(*length))),
        ],
        other => vec![format!("{name}: {}", c_type(other))],
    }
}

fn declaration(api: &Api, function: &Function, receiver: Option<&str>) -> String {
    let mut params = Vec::new();
    if let Some(object) = receiver {
        params.push(format!("this: *mut {object}"));
    }
    for param in &function.params {
        params.extend(c_params(api, param));
    }
    // `[HxnStatus]` functions return a C `int`.
    let returns = match (function.status, &function.returns) {
        (Status::Zero, _) => String::from(" -> c_int"),
        _ if function.ignored == Ignored::Int => String::from(" -> c_int"),
        _ if function.ignored == Ignored::Pointer => String::from(" -> *mut c_void"),
        (Status::None, _) if function.out().is_some() => String::new(),
        (_, Ty::Void) => String::new(),
        (_, Ty::Nullable(of)) if matches!(**of, Ty::Text) && function.free.is_some() => {
            String::from(" -> *const c_char")
        }
        (_, ty) => format!(" -> {}", c_type(ty)),
    };
    format!(
        "        pub fn {}({}){returns};\n",
        function.c,
        params.join(", ")
    )
}

/// The Rust bindings for `api`, the text of a module. `source` names the
/// WebIDL file it was generated from.
pub fn bindings(api: &Api, source: &str) -> String {
    let mut out = format!(
        "// Generated by hxn from {source}. Do not edit: edit the WebIDL, and the next\n// `hxn build` generates this again.\n\n#![allow(clippy::useless_conversion, clippy::unnecessary_cast, clippy::let_and_return, clippy::needless_question_mark, clippy::unnecessary_lazy_evaluations, clippy::redundant_closure_call, dead_code, unused_imports)]\n\nuse std::ffi::{{CStr, CString, c_char, c_int, c_long, c_ulong, c_void}};\nuse std::ptr::null_mut;\n"
    );
    // hxn reads a function's result as a stream only when it is spelled
    // `Stream<T>`.
    if !used(api, Role::Kept).is_empty() {
        out.push_str("use hxn_future::Stream;\n");
    }

    // The C declarations.
    out.push_str("\nmod ffi {\n    use super::*;\n");
    for object in &api.objects {
        out.push_str(&format!(
            "\n    #[repr(C)]\n    pub struct {} {{\n        _opaque: [u8; 0],\n    }}\n",
            object.name
        ));
    }
    for record in &api.structs {
        let fields: String = record
            .fields
            .iter()
            .map(|field| {
                format!(
                    "        pub {}: {},\n",
                    ident(&field.name),
                    c_type(&field.ty)
                )
            })
            .collect();
        out.push_str(&format!(
            "\n    #[repr(C)]\n    #[derive(Clone, Copy)]\n    pub struct {} {{\n{fields}    }}\n",
            record.name
        ));
    }
    out.push_str("\n    unsafe extern \"C\" {\n");
    let mut declared = BTreeSet::new();
    let mut declare = |out: &mut String, name: &str, text: String| {
        if declared.insert(name.to_owned()) {
            out.push_str(&text);
        }
    };
    for object in &api.objects {
        if let Some(free) = &object.free {
            declare(
                &mut out,
                free,
                format!(
                    "        pub fn {free}(this: *mut {}){};\n",
                    object.name,
                    if object.free_int { " -> c_int" } else { "" }
                ),
            );
        }
        if let Some(made) = &object.constructor {
            let mut function = made.clone();
            function.returns = Ty::Object(object.name.clone());
            declare(&mut out, &made.c, declaration(api, &function, None));
        }
        for method in &object.methods {
            declare(
                &mut out,
                &method.c,
                declaration(api, method, Some(&object.name)),
            );
        }
        for function in &object.statics {
            declare(&mut out, &function.c, declaration(api, function, None));
        }
    }
    for function in &api.functions {
        declare(&mut out, &function.c, declaration(api, function, None));
    }
    let frees: BTreeSet<&String> = api
        .objects
        .iter()
        .flat_map(|object| object.methods.iter().chain(&object.statics))
        .chain(&api.functions)
        .filter_map(|function| function.free.as_ref())
        .collect();
    for free in frees {
        declare(
            &mut out,
            free,
            format!("        pub fn {free}(text: *mut c_char);\n"),
        );
    }
    out.push_str("    }\n}\n");

    // Helpers.
    let lent = used(api, Role::In);
    if !lent.is_empty() {
        out.push_str(LENT);
        for known in lent {
            out.push_str(&trampoline(known));
        }
    }
    if all_functions(api).any(|f| f.params.iter().any(|p| p.role == Role::Frees)) {
        out.push_str(FREES);
    }
    let kept = used(api, Role::Kept);
    if !kept.is_empty() {
        out.push_str(KEPT);
        for known in kept {
            out.push_str(&keeper(known));
        }
    }
    out.push_str("\n// Text as C takes it. Text with a NUL in it cannot be passed.\nfn hxn_text(value: &str) -> Result<CString, String> {\n    CString::new(value).map_err(|_| String::from(\"text with a NUL character cannot be passed to C\"))\n}\n\n// A copy of the text C returned, or `None` for a null pointer.\n//\n// # Safety\n// `value` is null or points to text ending in a NUL.\nunsafe fn hxn_copy(value: *const c_char) -> Option<String> {\n    if value.is_null() {\n        return None;\n    }\n    Some(unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned())\n}\n");

    for known in &api.enums {
        let variants: String = known
            .values
            .iter()
            .map(|(name, value)| format!("    {name} = {value},\n"))
            .collect();
        let read: String = known
            .values
            .iter()
            .map(|(name, value)| format!("            {value} => Ok(Self::{name}),\n"))
            .collect();
        out.push_str(&format!(
            "\n{}#[hxn::data]\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum {name} {{\n{variants}}}\n\nimpl {name} {{\n    fn from_c(value: c_int) -> Result<Self, String> {{\n        match value {{\n{read}            other => Err(format!(\"{{other}} is not a {name}\")),\n        }}\n    }}\n}}\n",
            docs(&known.docs, ""),
            name = known.name,
        ));
    }

    for record in &api.structs {
        let name = &record.name;
        let fields: String = record
            .fields
            .iter()
            .map(|field| {
                format!(
                    "    pub {}: {},\n",
                    ident(&field.name),
                    rust_return(&field.ty)
                )
            })
            .collect();
        let to_c: String = record
            .fields
            .iter()
            .map(|field| {
                let field = ident(&field.name);
                format!("            {field}: value.{field} as _,\n")
            })
            .collect();
        let from_c: String = record
            .fields
            .iter()
            .map(|field| {
                let field = ident(&field.name);
                format!("            {field}: value.{field} as _,\n")
            })
            .collect();
        out.push_str(&format!(
            "\n{}#[hxn::data]\n#[derive(Clone, Copy, Debug, PartialEq)]\npub struct {name} {{\n{fields}}}\n\nimpl From<&{name}> for ffi::{name} {{\n    fn from(value: &{name}) -> Self {{\n        Self {{\n{to_c}        }}\n    }}\n}}\n\nimpl From<ffi::{name}> for {name} {{\n    fn from(value: ffi::{name}) -> Self {{\n        Self {{\n{from_c}        }}\n    }}\n}}\n",
            docs(&record.docs, ""),
        ));
    }

    for object in &api.objects {
        let name = &object.name;
        let kept = if keeps(object) {
            "    // The senders of the callbacks C keeps, dropped after C frees `raw`.\n    kept: std::sync::Mutex<Vec<Box<dyn Send>>>,\n"
        } else {
            ""
        };
        out.push_str(&format!(
            "\n{}#[hxn::interface]\npub struct {name} {{\n    raw: *mut ffi::{name},\n    // False for an object that belongs to another, which frees it.\n    owned: bool,\n{kept}}}\n\n{}",
            docs(&object.docs, ""),
            if object.thread_safe {
                String::from("// The C library allows calls on it from several threads at once.\nunsafe impl Send for ")
                    + name
                    + " {}\nunsafe impl Sync for "
                    + name
                    + " {}\n"
            } else {
                String::from("// Only moved between threads, never shared: every method takes `&mut self`.\nunsafe impl Send for ")
                    + name
                    + " {}\n"
            },
        ));
        if let Some(free) = &object.free {
            out.push_str(&format!(
                "\nimpl Drop for {name} {{\n    fn drop(&mut self) {{\n        if self.owned {{\n            unsafe {{ ffi::{free}(self.raw) }};\n        }}\n    }}\n}}\n"
            ));
        }
        let mut members = Vec::new();
        if let Some(made) = &object.constructor {
            members.push(wrapper(api, made, Receiver::None, false, ""));
        }
        for method in &object.methods {
            let receiver = if object.thread_safe {
                Receiver::Shared
            } else {
                Receiver::Exclusive
            };
            members.push(wrapper(api, method, receiver, keeps(object), ""));
        }
        for function in &object.statics {
            members.push(wrapper(api, function, Receiver::None, false, ""));
        }
        if !members.is_empty() {
            out.push_str(&format!(
                "\n#[hxn::methods]\nimpl {name} {{\n{}}}\n",
                members.join("\n")
            ));
        }
    }

    if !api.functions.is_empty() {
        let class = namespace(&api.library);
        let marker = if api.objects.iter().any(|object| object.name == class) {
            // Statics of the object of the same name.
            String::from("#[hxn::function]\n")
        } else {
            format!("#[hxn::function(class = \"{class}\")]\n")
        };
        for function in &api.functions {
            let text = wrapper(api, function, Receiver::None, false, &marker);
            // A free function is not indented.
            let text: String = text
                .lines()
                .map(|line| line.strip_prefix("    ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n");
            out.push('\n');
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}
