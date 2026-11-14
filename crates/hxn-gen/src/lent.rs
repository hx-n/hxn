//! Haxe callbacks, borrowed for the duration of a call.
//!
//! A library function accepts one as `impl Fn(..) -> Result<T, E>`. It is
//! invoked during the call, on the calling thread, and it can fail, because
//! the Haxe function can throw. An exception cannot unwind through native
//! code, so the Haxe side wraps the function the user passed: the wrapper
//! catches and stores the exception and returns a sentinel value, the
//! trampoline turns that into `Status::CALLBACK`, the library sees an `Err`
//! and returns, and the binding rethrows the stored exception afterwards.
//!
//! Over the C ABI a callback is an `hxn_callback`; on HashLink it is a
//! closure, called through a trampoline that boxes its arguments.
//!
//! Text and bytes are passed to a callback borrowed, as a slice the host
//! copies during the call, and are returned owned: a buffer the host fills
//! and the library frees. An object passed to a callback is transferred: the
//! host owns it from then on, as it owns an object a function returns. Data
//! is passed and returned in its usual wire format, an enum's integer value
//! or the serialized bytes, which the adapter's closure writes and reads.

use crate::data;
use crate::model::{Data, Library, Type};

/// A callback's parameter types and return type, if `ty` is a callback.
pub fn of(ty: &Type) -> Option<(&[Type], &Type)> {
    match ty {
        Type::Lent { params, returns } => Some((params, returns)),
        _ => None,
    }
}

/// Each callback shape (its signature at the boundary) the library accepts
/// somewhere, in order of first use. Two that differ only in an object's
/// interface are one shape: at the boundary every object is a pointer.
pub fn used(library: &Library) -> Vec<&Type> {
    let mut out: Vec<&Type> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for method in library.interfaces.iter().flat_map(|i| &i.methods) {
        for param in &method.params {
            let Some((params, returns)) = of(&param.ty) else {
                continue;
            };
            let key = key(params, returns);
            if !seen.contains(&key) {
                seen.push(key);
                out.push(&param.ty);
            }
        }
    }
    out
}

/// The interfaces whose objects are passed to a callback, in order of first
/// use.
pub fn given(library: &Library) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for method in library.interfaces.iter().flat_map(|i| &i.methods) {
        for param in &method.params {
            let Some((params, _)) = of(&param.ty) else {
                continue;
            };
            for taken in params {
                if let Type::Handle { interface, .. } = taken
                    && !out.contains(&interface.as_str())
                {
                    out.push(interface);
                }
            }
        }
    }
    out
}

// A type's wire type: the type itself, or its data's wire type. Two shapes
// that differ only in their data types are one shape.
fn crossed(ty: &Type) -> Type {
    match ty {
        Type::Data(data) => data.wire().unwrap_or(Type::Bytes),
        other => other.clone(),
    }
}

fn letter(ty: &Type) -> char {
    match crossed(ty) {
        Type::Void => 'v',
        Type::Bool => 'b',
        Type::I32 => 'i',
        Type::F64 => 'd',
        Type::Str => 's',
        Type::Bytes => 'y',
        Type::Handle { .. } => 'o',
        _ => unreachable!("not passed to or from a lent function"),
    }
}

/// The runtime a Haxe class is generated for: runtimes differ in what a
/// callback's wrapper receives for text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    Hl,
    Cpp,
}

// A shape that passes more than numbers is called through a dedicated
// function: the standard `invoke` cannot take a borrowed argument or convert
// a return value.
fn texts(params: &[Type], returns: &Type) -> bool {
    params
        .iter()
        .any(|ty| matches!(crossed(ty), Type::Str | Type::Bytes | Type::Handle { .. }))
        || matches!(crossed(returns), Type::Str | Type::Bytes)
}

// The function such a shape is called through: `call_si_to_s`.
fn caller(params: &[Type], returns: &Type) -> String {
    format!("call_{}", key(params, returns))
}

// A shape's name within a function name: `id_to_b`, or `to_i` for one with
// no parameters.
fn key(params: &[Type], returns: &Type) -> String {
    let taken: String = params.iter().map(letter).collect();
    if taken.is_empty() {
        format!("to_{}", letter(returns))
    } else {
        format!("{taken}_to_{}", letter(returns))
    }
}

/// The trampoline a shape is called through: `lent_id_to_b`.
pub fn trampoline(params: &[Type], returns: &Type) -> String {
    format!("lent_{}", key(params, returns))
}

/// The Rust name of a shape's function pointer type: `LentIDToB`.
pub fn alias(params: &[Type], returns: &Type) -> String {
    let taken: String = params
        .iter()
        .map(|ty| letter(ty).to_ascii_uppercase())
        .collect();
    format!("Lent{taken}To{}", letter(returns).to_ascii_uppercase())
}

// An argument's type at the boundary: a bool is an int, text and bytes are
// borrowed for the call, and an object is the pointer the host will own.
fn c_type(ty: &Type) -> &'static str {
    match crossed(ty) {
        Type::F64 => "f64",
        Type::Str | Type::Bytes => "hxn_abi::Slice<'_>",
        Type::Handle { .. } => "*mut c_void",
        _ => "i32",
    }
}

// A return value's type at the boundary: a callback that returns nothing
// writes an int that is never read, and text or bytes are a copy made by the
// host for the library to free.
fn c_given(ty: &Type) -> &'static str {
    match crossed(ty) {
        Type::F64 => "f64",
        Type::Str | Type::Bytes => "hxn_abi::Buf",
        _ => "i32",
    }
}

fn rust_type(ty: &Type) -> &'static str {
    match crossed(ty) {
        Type::Void => "()",
        Type::Bool => "bool",
        Type::F64 => "f64",
        Type::Str => "String",
        Type::Bytes => "Vec<u8>",
        _ => "i32",
    }
}

/// The function pointer types, for either adapter.
pub fn aliases(library: &Library) -> String {
    let mut out = String::new();
    for shape in used(library) {
        let (params, returns) = of(shape).expect("a lent function");
        let taken: String = params
            .iter()
            .map(|ty| format!("{}, ", c_type(ty)))
            .collect();
        out.push_str(&format!(
            "\ntype {} = unsafe extern \"C\" fn(*mut c_void, {taken}*mut {}) -> Status;\n",
            alias(params, returns),
            c_given(returns)
        ));
        if texts(params, returns) {
            out.push_str(&call(params, returns));
        }
    }
    out
}

// The function a shape with text is called through. The host's output is
// read only after it reports success.
fn call(params: &[Type], returns: &Type) -> String {
    let declared: String = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match crossed(ty) {
            Type::Str => format!(", a{n}: &str"),
            Type::Bytes => format!(", a{n}: &[u8]"),
            // Already the pointer passed to the host.
            Type::Handle { .. } => format!(", a{n}: *mut c_void"),
            other => format!(", a{n}: {}", rust_type(&other)),
        })
        .collect();
    let passed: String = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match crossed(ty) {
            Type::Bool => format!("i32::from(a{n}), "),
            Type::Str | Type::Bytes => format!("hxn_abi::Slice::from(a{n}), "),
            _ => format!("a{n}, "),
        })
        .collect();
    let given = match crossed(returns) {
        Type::Void => "let _ = out;\n    Ok(())",
        Type::Bool => "Ok(out != 0)",
        Type::Str => "Ok(String::from_utf8(out.as_bytes().to_vec()).map_err(|_| Fault::Encoding)?)",
        Type::Bytes => "Ok(out.as_bytes().to_vec())",
        _ => "Ok(out)",
    };
    format!(
        "\n#[allow(clippy::too_many_arguments)]\nunsafe fn {name}(callback: &Callback<{alias}>{declared}) -> Result<{returns}, hxn_abi::Failure> {{\n    let call = callback.func()?;\n    let mut out = std::mem::MaybeUninit::<{given_type}>::uninit();\n    let status = unsafe {{ call(callback.env(), {passed}out.as_mut_ptr()) }};\n    if !status.is_ok() {{\n        return Err(hxn_abi::Failure::new(status, \"callback failed\"));\n    }}\n    let out = unsafe {{ out.assume_init() }};\n    {given}\n}}\n",
        name = caller(params, returns),
        alias = alias(params, returns),
        returns = rust_type(returns),
        given_type = c_given(returns),
    )
}

/// The closure an adapter passes to the library's function for the callback
/// `name`: it invokes the callback, and returns an `Err` to the library when
/// the Haxe function threw.
pub fn closure(host: Host, name: &str, params: &[Type], returns: &Type) -> String {
    let names: Vec<String> = (0..params.len()).map(|n| format!("a{n}")).collect();
    if params
        .iter()
        .chain([returns])
        .any(|ty| matches!(ty, Type::Data(_)))
    {
        return data_closure(host, name, params, returns, &names);
    }
    if texts(params, returns) {
        // An object is transferred here, as the handle the host will own.
        let passed: String = params
            .iter()
            .zip(&names)
            .map(|(ty, name)| match (ty, host) {
                (Type::Handle { .. }, Host::Cpp) => format!(", Handle::into_raw({name}).cast()"),
                (Type::Handle { .. }, Host::Hl) => format!(", manage({name}).cast()"),
                _ => format!(", {name}"),
            })
            .collect();
        // Inside the adapter's `unsafe` block, like every call here.
        return format!(
            "|{}| {}(&{name}{passed})",
            names.join(", "),
            caller(params, returns)
        );
    }
    let passed: Vec<String> = params
        .iter()
        .zip(&names)
        .map(|(ty, name)| match ty {
            Type::Bool => format!("i32::from({name})"),
            _ => name.clone(),
        })
        .collect();
    let back = match returns {
        Type::Void => ".map(|_| ())",
        Type::Bool => ".map(|out| out != 0)",
        _ => "",
    };
    format!(
        "|{}| {name}.invoke({}){back}",
        names.join(", "),
        passed.join(", ")
    )
}

// The closure for a shape that passes data. Each data argument is serialized
// to its wire type before the call, whether the library passes it by value or
// by reference, and a data return value is deserialized after it.
fn data_closure(
    host: Host,
    name: &str,
    params: &[Type],
    returns: &Type,
    names: &[String],
) -> String {
    let mut before = String::new();
    let mut passed = Vec::new();
    for (ty, a) in params.iter().zip(names) {
        match (ty, host) {
            (Type::Data(Data::Enum(known)), _) => {
                before.push_str(&format!(
                    "let {a} = {}(std::borrow::Borrow::<{known}>::borrow(&{a})); ",
                    data::writer(known)
                ));
                passed.push(a.clone());
            }
            (Type::Data(data), _) => {
                before.push_str(&format!(
                    "let {a} = {{ let part = std::borrow::Borrow::<{}>::borrow(&{a}); let mut bytes = Vec::new(); {{ let out = &mut bytes; {} }} bytes }}; ",
                    data::rust_lent(data),
                    data::rust_write(data, "part")
                ));
                passed.push(format!("&{a}"));
            }
            (Type::Handle { .. }, Host::Cpp) => {
                passed.push(format!("Handle::into_raw({a}).cast()"))
            }
            (Type::Handle { .. }, Host::Hl) => passed.push(format!("manage({a}).cast()")),
            (Type::Bool, _) if !texts(params, returns) => passed.push(format!("i32::from({a})")),
            _ => passed.push(a.clone()),
        }
    }
    let called = if texts(params, returns) {
        let passed: String = passed.iter().map(|a| format!(", {a}")).collect();
        format!("{}(&{name}{passed})", caller(params, returns))
    } else {
        let back = match crossed(returns) {
            Type::Void => ".map(|_| ())",
            Type::Bool => ".map(|out| out != 0)",
            _ => "",
        };
        format!("{name}.invoke({}){back}", passed.join(", "))
    };
    let given = match returns {
        Type::Data(Data::Enum(known)) => format!("{}(out)?", data::reader(known)),
        Type::Data(data) => data::rust_decoded(data, "&out"),
        _ => return format!("|{}| {{ {before}{called} }}", names.join(", ")),
    };
    format!(
        "|{}| {{ {before}let out = {called}?; Ok::<_, hxn_abi::Failure>({given}) }}",
        names.join(", ")
    )
}

/// The closure's type, in HashLink primitive signature notation.
pub fn hl_signature(params: &[Type], returns: &Type) -> String {
    // Text reaches the wrapper as VM bytes, which it converts to a `String`;
    // bytes come with their length; and an object is a pointer whose type the
    // wrapper knows.
    let taken: String = params
        .iter()
        .map(|ty| match crossed(ty) {
            Type::Str | Type::Handle { .. } => String::from("B"),
            Type::Bytes => String::from("Bi"),
            other => letter(&other).to_string(),
        })
        .collect();
    let given = match crossed(returns) {
        // The wrapper returns whether the function returned without throwing.
        Type::Void => String::from("b"),
        // A `String`, which is null when it threw.
        Type::Str => String::from("OBi_"),
        // A `haxe.io.Bytes`, which is null when it threw.
        Type::Bytes => String::from("OiB_"),
        other => format!("N{}", letter(&other)),
    };
    format!("P{taken}_{given}")
}

/// The trampolines HashLink closures are called through.
pub fn hl_trampolines(library: &Library) -> String {
    let mut out = String::new();
    for shape in used(library) {
        let (params, returns) = of(shape).expect("a lent function");
        let taken: String = params
            .iter()
            .enumerate()
            .map(|(n, ty)| format!("a{n}: {}, ", c_type(ty)))
            .collect();
        // Text and bytes are checked before anything is called, and copied
        // for the VM.
        let prepared: String = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match crossed(ty) {
                Type::Str => format!(
                    "    let Ok(a{n}) = a{n}.as_str() else {{\n        return Status::ENCODING;\n    }};\n"
                ),
                Type::Bytes => format!(
                    "    let Ok(a{n}) = a{n}.as_bytes() else {{\n        return Status::NULL;\n    }};\n    let Ok(a{n}_length) = i32::try_from(a{n}.len()) else {{\n        return Status::TOO_LARGE;\n    }};\n"
                ),
                _ => String::new(),
            })
            .collect();
        let boxed: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match crossed(ty) {
                Type::Bool => format!("box_bool(a{n} != 0)"),
                Type::F64 => format!("box_f64(a{n})"),
                Type::Str => format!("box_bytes(utf16(a{n}))"),
                Type::Bytes => format!("box_bytes(bytes(a{n})), box_i32(a{n}_length)"),
                // The VM's pointer to the object, passed as bytes, because a
                // closure called this way cannot receive a typed pointer. The
                // wrapper restores its type with a primitive.
                Type::Handle { .. } => format!("box_bytes(a{n}.cast())"),
                _ => format!("box_i32(a{n})"),
            })
            .collect();
        let finish = if crossed(returns) == Type::Str {
            String::from(
                "    match unsafe { string(call(env.cast(), &mut args).cast()) } {\n        Ok(Some(result)) => {\n            unsafe { out.write(hxn_abi::Buf::from(result.into_bytes())) };\n            Status::OK\n        }\n        Ok(None) => Status::CALLBACK,\n        Err(_) => Status::ENCODING,\n    }\n",
            )
        } else if crossed(returns) == Type::Bytes {
            String::from(
                "    match unsafe { buffer(call(env.cast(), &mut args).cast()) } {\n        Some(result) => {\n            unsafe { out.write(hxn_abi::Buf::from(result)) };\n            Status::OK\n        }\n        None => Status::CALLBACK,\n    }\n",
            )
        } else {
            let (unbox, pattern, written) = match crossed(returns) {
                Type::Void => ("unbox_bool", "Some(true)", "0"),
                Type::Bool => ("unbox_bool", "Some(result)", "i32::from(result)"),
                Type::F64 => ("unbox_f64", "Some(result)", "result"),
                _ => ("unbox_i32", "Some(result)", "result"),
            };
            format!(
                "    match unsafe {{ {unbox}(call(env.cast(), &mut args)) }} {{\n        {pattern} => {{\n            unsafe {{ out.write({written}) }};\n            Status::OK\n        }}\n        _ => Status::CALLBACK,\n    }}\n"
            )
        };
        out.push_str(&format!(
            "\n// The binding's wrapper gives null, or false, when the Haxe function threw.\nunsafe extern \"C\" fn {name}(env: *mut c_void, {taken}out: *mut {ty}) -> Status {{\n{prepared}    let mut args: [*mut vdynamic; {count}] = [{boxed}];\n{finish}}}\n",
            name = trampoline(params, returns),
            ty = c_given(returns),
            count = params.len()
                + params
                    .iter()
                    .filter(|ty| crossed(ty) == Type::Bytes)
                    .count(),
            boxed = boxed.join(", "),
        ));
    }
    out
}

/// The same for hxcpp, in its glue.
pub fn cpp_trampolines(library: &Library) -> String {
    let mut out = String::new();
    for shape in used(library) {
        let (params, returns) = of(shape).expect("a lent function");
        let taken: String = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match crossed(ty) {
                Type::F64 => format!("double a{n}, "),
                Type::Str | Type::Bytes => format!("hxn_slice a{n}, "),
                Type::Handle { .. } => format!("void *a{n}, "),
                _ => format!("int a{n}, "),
            })
            .collect();
        let passed: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match crossed(ty) {
                Type::Bool => format!("a{n} != 0"),
                Type::Str => format!("hxn::text(a{n})"),
                Type::Bytes => format!("hxn::bytes(a{n})"),
                Type::Handle { .. } => format!("cpp::Pointer<void>(a{n})"),
                _ => format!("a{n}"),
            })
            .collect();
        let (ty, failed, written) = match crossed(returns) {
            Type::Void => ("int", "result == null() || !(bool)result", "*out = 0;"),
            Type::Bool => ("int", "result == null()", "*out = (bool)result ? 1 : 0;"),
            Type::F64 => ("double", "result == null()", "*out = (double)result;"),
            // A copy for the library to free, since the string is owned by
            // the garbage collector.
            Type::Str => (
                "hxn_buf",
                "result == null()",
                "if (!hxn::give_text((::String)result, out))\n\t\treturn HXN_TOO_LARGE;",
            ),
            Type::Bytes => (
                "hxn_buf",
                "result == null()",
                "if (!hxn::give_bytes((Array<unsigned char>)result, out))\n\t\treturn HXN_TOO_LARGE;",
            ),
            _ => ("int", "result == null()", "*out = (int)result;"),
        };
        out.push_str(&format!(
            "\n// The binding's wrapper gives null, or false, when the Haxe function threw.\ninline hxn_status {name}(void *env, {taken}{ty} *out) {{\n\tDynamic result = Dynamic((hx::Object *)env)({passed});\n\tif ({failed})\n\t\treturn HXN_CALLBACK;\n\t{written}\n\treturn HXN_OK;\n}}\n",
            name = trampoline(params, returns),
            passed = passed.join(", "),
        ));
    }
    out
}

fn haxe_scalar(ty: &Type) -> String {
    match ty {
        Type::Void => String::from("Void"),
        Type::Bool => String::from("Bool"),
        Type::I32 => String::from("Int"),
        Type::F64 => String::from("Float"),
        Type::Str => String::from("String"),
        Type::Bytes => String::from("haxe.io.Bytes"),
        Type::Handle { interface, .. } => interface.clone(),
        Type::Data(data) => data::haxe_type(data),
        _ => unreachable!("not passed to or from a lent function"),
    }
}

fn haxe_taken(params: &[Type]) -> String {
    let taken: Vec<String> = params.iter().map(haxe_scalar).collect();
    taken.join(", ")
}

// The types the wrapper receives for an argument, each declared as its own
// parameter. HashLink native code cannot create a `String` or a
// `haxe.io.Bytes`, so on HashLink both reach the wrapper as VM bytes, bytes
// with their length, and so does an object's pointer.
fn haxe_handed(host: Host, ty: &Type) -> Vec<String> {
    match (host, crossed(ty)) {
        (Host::Hl, Type::Bytes) => vec![String::from("hl.Bytes"), String::from("Int")],
        (Host::Hl, Type::Str | Type::Handle { .. }) => vec![String::from("hl.Bytes")],
        (Host::Cpp, Type::Bytes) => vec![String::from("haxe.io.BytesData")],
        (Host::Cpp, Type::Handle { .. }) => vec![String::from("cpp.Pointer<cpp.Void>")],
        (_, other) => vec![haxe_scalar(&other)],
    }
}

fn haxe_declared(host: Host, n: usize, ty: &Type) -> String {
    let types = haxe_handed(host, ty);
    match types.as_slice() {
        [bytes, length] => format!("a{n}:{bytes}, a{n}Length:{length}"),
        types => format!("a{n}:{}", types[0]),
    }
}

/// The type of a Haxe method's callback parameter: `(Int, Float) -> Bool`.
pub fn haxe_type(params: &[Type], returns: &Type) -> String {
    format!("({}) -> {}", haxe_taken(params), haxe_scalar(returns))
}

// The wrapper's return type: the value or null, or whether it returned
// without throwing. HashLink native code reads a `haxe.io.Bytes` directly;
// hxcpp native code receives its underlying data.
fn haxe_guarded(host: Host, returns: &Type) -> String {
    match (host, crossed(returns)) {
        (_, Type::Void) => String::from("Bool"),
        (Host::Hl, Type::Bytes) => String::from("haxe.io.Bytes"),
        (Host::Cpp, Type::Bytes) => String::from("haxe.io.BytesData"),
        (_, other) => format!("Null<{}>", haxe_scalar(&other)),
    }
}

/// The type of the wrapper, which is what native code receives.
pub fn haxe_native_type(host: Host, params: &[Type], returns: &Type) -> String {
    let handed: Vec<String> = params.iter().flat_map(|ty| haxe_handed(host, ty)).collect();
    format!("({}) -> {}", handed.join(", "), haxe_guarded(host, returns))
}

/// The name of the wrapper around the function `name`.
pub fn guarded(name: &str) -> String {
    format!("{name}Guarded")
}

/// The statement that wraps the callback `name`: the wrapper catches and
/// stores an exception and returns a value that tells the library the call
/// failed.
pub fn haxe_guard(host: Host, name: &str, params: &[Type], returns: &Type) -> String {
    let declared: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| haxe_declared(host, n, ty))
        .collect();
    // An object is given an owner before anything can throw, so it is
    // released whatever the function does with it.
    let owned: String = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match (host, ty) {
            (Host::Hl, Type::Handle { interface, .. }) => {
                format!("\n\t\t\tvar o{n} = {interface}.hxnAdoptLent(a{n});")
            }
            (Host::Cpp, Type::Handle { interface, .. }) => format!(
                "\n\t\t\tvar o{n} = {interface}.hxnAdopt();\n\t\t\to{n}.hxnRaw = a{n}.raw;\n\t\t\to{n}.hxnOwn();"
            ),
            _ => String::new(),
        })
        .collect();
    // Data is deserialized inside the `try`, one statement per argument, in
    // order.
    let mut read = String::new();
    let passed: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| {
            let bytes = match host {
                Host::Hl => format!("@:privateAccess new haxe.io.Bytes(a{n}, a{n}Length)"),
                Host::Cpp => format!("haxe.io.Bytes.ofData(a{n})"),
            };
            match (host, ty) {
                (Host::Hl, Type::Str) => format!("@:privateAccess String.fromUCS2(a{n})"),
                (_, Type::Bytes) => bytes,
                (_, Type::Handle { .. }) => format!("o{n}"),
                (_, Type::Data(data)) if crossed(ty) == Type::Bytes => {
                    read.push_str(&format!(
                        "var d{n}:{} = {{\n\t\t\t\t\tvar input = new haxe.io.BytesInput({bytes});\n\t\t\t\t\t{};\n\t\t\t\t}};\n\t\t\t\t",
                        data::haxe_type(data),
                        data::haxe_read(data)
                    ));
                    format!("d{n}")
                }
                _ => format!("a{n}"),
            }
        })
        .collect();
    let call = format!("{name}({})", passed.join(", "));
    // What the runtime receives for bytes the function returned.
    let handed = |bytes: &str| match host {
        Host::Hl => bytes.to_owned(),
        Host::Cpp => format!("{bytes}.getData()"),
    };
    let (succeed, fail) = match returns {
        Type::Void => (format!("{call};\n\t\t\t\treturn true;"), "false"),
        Type::Bytes => (
            format!(
                "var result = {call};\n\t\t\t\tif (result == null)\n\t\t\t\t\tthrow new haxe.Exception(\"a function lent to a library returned null\");\n\t\t\t\treturn {};",
                handed("result")
            ),
            "null",
        ),
        Type::Data(data) if crossed(returns) == Type::Bytes => {
            let mut lines = vec![
                format!("var result = {call};"),
                String::from("var out = new haxe.io.BytesBuffer();"),
            ];
            lines.extend(data::haxe_write(data, "result", "out", 0));
            lines.push(format!("return {};", handed("out.getBytes()")));
            (lines.join("\n").replace('\n', "\n\t\t\t\t"), "null")
        }
        // Null is how the wrapper reports a throw, so it is not a valid
        // return value.
        Type::Str => (
            format!(
                "var result = {call};\n\t\t\t\tif (result == null)\n\t\t\t\t\tthrow new haxe.Exception(\"a function lent to a library returned null\");\n\t\t\t\treturn result;"
            ),
            "null",
        ),
        _ => (format!("return {call};"), "null"),
    };
    format!(
        "var {guarded} = function({declared}):{ty} {{{owned}\n\t\t\ttry {{\n\t\t\t\t{read}{succeed}\n\t\t\t}} catch (error:haxe.Exception) {{\n\t\t\t\thxn.Callback.failed(error);\n\t\t\t\treturn {fail};\n\t\t\t}}\n\t\t}};",
        guarded = guarded(name),
        declared = declared.join(", "),
        ty = haxe_guarded(host, returns),
    )
}
