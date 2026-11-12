//! Haxe functions lent to a call.
//!
//! A library function takes one as `impl Fn(..) -> Result<T, E>`. It is
//! called during the call, on the calling thread, and it can fail, because
//! the Haxe function can throw. Native code cannot be thrown through, so the
//! Haxe side wraps what the user passed: the wrapper keeps the exception and
//! returns a value that marks it, the trampoline turns that into
//! `Status::CALLBACK`, the library sees an `Err` and returns, and the binding
//! throws the kept exception afterwards.
//!
//! Over the C ABI a lent function is an `hxn_callback`; to HashLink it is a
//! closure, called through a trampoline that boxes its arguments.
//!
//! Text and bytes go to one borrowed, as a slice the host copies during the
//! call, and text comes back owned: a buffer the host fills and the library
//! frees. An object goes to one for good: the host owns it from then on, as
//! it owns one a function returns.

use crate::model::{Library, Type};

/// What a lent function takes and gives, if `ty` is one.
pub fn of(ty: &Type) -> Option<(&[Type], &Type)> {
    match ty {
        Type::Lent { params, returns } => Some((params, returns)),
        _ => None,
    }
}

/// Each shape of function the library is lent somewhere, in the order met.
/// Two that differ only in which interface an object is of are one shape:
/// at the boundary an object is a pointer whatever it is.
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

/// The interfaces a lent function is given an object of, in the order met.
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

fn letter(ty: &Type) -> char {
    match ty {
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

/// Which runtime a Haxe class is written for: they differ in what a
/// function's wrapper is handed for text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    Hl,
    Cpp,
}

// A shape that passes more than numbers is called through a function of its
// own: the conventional `invoke` has no place for a borrowed argument or a
// result that is converted.
fn texts(params: &[Type], returns: &Type) -> bool {
    params
        .iter()
        .any(|ty| matches!(ty, Type::Str | Type::Bytes | Type::Handle { .. }))
        || *returns == Type::Str
}

// The function such a shape is called through: `call_si_to_s`.
fn caller(params: &[Type], returns: &Type) -> String {
    format!("call_{}", key(params, returns))
}

// A shape's name in a function: `id_to_b`, or `to_i` for one taking nothing.
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

// What an argument is at the boundary: a bool is an int, text and bytes are
// borrowed for the call, and an object is the pointer the host will own.
fn c_type(ty: &Type) -> &'static str {
    match ty {
        Type::F64 => "f64",
        Type::Str | Type::Bytes => "hxn_abi::Slice<'_>",
        Type::Handle { .. } => "*mut c_void",
        _ => "i32",
    }
}

// What a result is there: a function that returns nothing writes an int
// nobody reads, and text is the host's copy for the library to free.
fn c_given(ty: &Type) -> &'static str {
    match ty {
        Type::F64 => "f64",
        Type::Str => "hxn_abi::Buf",
        _ => "i32",
    }
}

fn rust_type(ty: &Type) -> &'static str {
    match ty {
        Type::Void => "()",
        Type::Bool => "bool",
        Type::F64 => "f64",
        Type::Str => "String",
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

// The function a shape with text is called through. What the host wrote is
// read only after it reported success.
fn call(params: &[Type], returns: &Type) -> String {
    let declared: String = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match ty {
            Type::Str => format!(", a{n}: &str"),
            Type::Bytes => format!(", a{n}: &[u8]"),
            // Already the pointer the host is handed.
            Type::Handle { .. } => format!(", a{n}: *mut c_void"),
            other => format!(", a{n}: {}", rust_type(other)),
        })
        .collect();
    let passed: String = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match ty {
            Type::Bool => format!("i32::from(a{n}), "),
            Type::Str | Type::Bytes => format!("hxn_abi::Slice::from(a{n}), "),
            _ => format!("a{n}, "),
        })
        .collect();
    let given = match returns {
        Type::Void => "let _ = out;\n    Ok(())",
        Type::Bool => "Ok(out != 0)",
        Type::Str => "Ok(String::from_utf8(out.as_bytes().to_vec()).map_err(|_| Fault::Encoding)?)",
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

/// The closure an adapter passes the library's function for the callback
/// `name`: it calls through, and gives the library an `Err` when the Haxe
/// function failed.
pub fn closure(host: Host, name: &str, params: &[Type], returns: &Type) -> String {
    let names: Vec<String> = (0..params.len()).map(|n| format!("a{n}")).collect();
    if texts(params, returns) {
        // An object is given away here, as the handle the host will own.
        let passed: String = params
            .iter()
            .zip(&names)
            .map(|(ty, name)| match (ty, host) {
                (Type::Handle { .. }, Host::Cpp) => format!(", Handle::into_raw({name}).cast()"),
                (Type::Handle { .. }, Host::Hl) => format!(", manage({name}).cast()"),
                _ => format!(", {name}"),
            })
            .collect();
        // Inside the adapter's own `unsafe` block, as every call here is.
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

/// The closure's type as a HashLink primitive's signature writes it.
pub fn hl_signature(params: &[Type], returns: &Type) -> String {
    // Text reaches the wrapper as the VM's bytes, which it makes a `String`
    // of; bytes come with their length; and an object is a pointer the
    // wrapper knows the type of.
    let taken: String = params
        .iter()
        .map(|ty| match ty {
            Type::Str | Type::Handle { .. } => String::from("B"),
            Type::Bytes => String::from("Bi"),
            other => letter(other).to_string(),
        })
        .collect();
    let given = match returns {
        // The wrapper's result is whether the function returned.
        Type::Void => String::from("b"),
        // A `String`, which is null when it threw.
        Type::Str => String::from("OBi_"),
        other => format!("N{}", letter(other)),
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
            .map(|(n, ty)| match ty {
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
            .map(|(n, ty)| match ty {
                Type::Bool => format!("box_bool(a{n} != 0)"),
                Type::F64 => format!("box_f64(a{n})"),
                Type::Str => format!("box_bytes(utf16(a{n}))"),
                Type::Bytes => format!("box_bytes(bytes(a{n})), box_i32(a{n}_length)"),
                // The VM's own pointer to the object, passed as bytes: a
                // closure called this way is not given a typed pointer. The
                // wrapper gets its type back from a primitive.
                Type::Handle { .. } => format!("box_bytes(a{n}.cast())"),
                _ => format!("box_i32(a{n})"),
            })
            .collect();
        let finish = if *returns == Type::Str {
            String::from(
                "    match unsafe { string(call(env.cast(), &mut args).cast()) } {\n        Ok(Some(result)) => {\n            unsafe { out.write(hxn_abi::Buf::from(result.into_bytes())) };\n            Status::OK\n        }\n        Ok(None) => Status::CALLBACK,\n        Err(_) => Status::ENCODING,\n    }\n",
            )
        } else {
            let (unbox, pattern, written) = match returns {
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
            count = params.len() + params.iter().filter(|ty| **ty == Type::Bytes).count(),
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
            .map(|(n, ty)| match ty {
                Type::F64 => format!("double a{n}, "),
                Type::Str | Type::Bytes => format!("hxn_slice a{n}, "),
                Type::Handle { .. } => format!("void *a{n}, "),
                _ => format!("int a{n}, "),
            })
            .collect();
        let passed: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match ty {
                Type::Bool => format!("a{n} != 0"),
                Type::Str => format!("hxn::text(a{n})"),
                Type::Bytes => format!("hxn::bytes(a{n})"),
                Type::Handle { .. } => format!("cpp::Pointer<void>(a{n})"),
                _ => format!("a{n}"),
            })
            .collect();
        let (ty, failed, written) = match returns {
            Type::Void => ("int", "result == null() || !(bool)result", "*out = 0;"),
            Type::Bool => ("int", "result == null()", "*out = (bool)result ? 1 : 0;"),
            Type::F64 => ("double", "result == null()", "*out = (double)result;"),
            // A copy the library frees, since the string is the collector's.
            Type::Str => (
                "hxn_buf",
                "result == null()",
                "if (!hxn::give_text((::String)result, out))\n\t\treturn HXN_TOO_LARGE;",
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
        _ => unreachable!("not passed to or from a lent function"),
    }
}

fn haxe_taken(params: &[Type]) -> String {
    let taken: Vec<String> = params.iter().map(haxe_scalar).collect();
    taken.join(", ")
}

// What the wrapper is handed for an argument: the types, and each with the
// name it is declared by. HashLink's native code cannot make a `String` or a
// `haxe.io.Bytes`, so both reach the wrapper there as the VM's bytes, bytes
// with their length, and so does an object's pointer.
fn haxe_handed(host: Host, ty: &Type) -> Vec<String> {
    match (host, ty) {
        (Host::Hl, Type::Bytes) => vec![String::from("hl.Bytes"), String::from("Int")],
        (Host::Hl, Type::Str | Type::Handle { .. }) => vec![String::from("hl.Bytes")],
        (Host::Cpp, Type::Bytes) => vec![String::from("haxe.io.BytesData")],
        (Host::Cpp, Type::Handle { .. }) => vec![String::from("cpp.Pointer<cpp.Void>")],
        (_, other) => vec![haxe_scalar(other)],
    }
}

fn haxe_declared(host: Host, n: usize, ty: &Type) -> String {
    let types = haxe_handed(host, ty);
    match types.as_slice() {
        [bytes, length] => format!("a{n}:{bytes}, a{n}Length:{length}"),
        types => format!("a{n}:{}", types[0]),
    }
}

/// The type of the function a Haxe method takes: `(Int, Float) -> Bool`.
pub fn haxe_type(params: &[Type], returns: &Type) -> String {
    format!("({}) -> {}", haxe_taken(params), haxe_scalar(returns))
}

// What the wrapper returns: the result or null, or whether it returned.
fn haxe_guarded(returns: &Type) -> String {
    match returns {
        Type::Void => String::from("Bool"),
        other => format!("Null<{}>", haxe_scalar(other)),
    }
}

/// The type of the wrapper, which is what native code is given.
pub fn haxe_native_type(host: Host, params: &[Type], returns: &Type) -> String {
    let handed: Vec<String> = params.iter().flat_map(|ty| haxe_handed(host, ty)).collect();
    format!("({}) -> {}", handed.join(", "), haxe_guarded(returns))
}

/// The name of the wrapper around the function `name`.
pub fn guarded(name: &str) -> String {
    format!("{name}Guarded")
}

/// The statement that wraps the function `name`: the wrapper catches,
/// keeps the exception and returns what tells the library the call failed.
pub fn haxe_guard(host: Host, name: &str, params: &[Type], returns: &Type) -> String {
    let declared: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| haxe_declared(host, n, ty))
        .collect();
    // An object is owned before anything can throw, so it is released
    // whatever the function does with it.
    let owned: String = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match (host, ty) {
            (Host::Hl, Type::Handle { interface, .. }) => {
                format!("\n\t\t\tvar o{n} = {interface}.adoptLent(a{n});")
            }
            (Host::Cpp, Type::Handle { interface, .. }) => format!(
                "\n\t\t\tvar o{n} = {interface}.adopt();\n\t\t\to{n}.raw = a{n}.raw;\n\t\t\to{n}.own();"
            ),
            _ => String::new(),
        })
        .collect();
    let passed: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| match (host, ty) {
            (Host::Hl, Type::Str) => format!("@:privateAccess String.fromUCS2(a{n})"),
            (Host::Hl, Type::Bytes) => {
                format!("@:privateAccess new haxe.io.Bytes(a{n}, a{n}Length)")
            }
            (Host::Cpp, Type::Bytes) => format!("haxe.io.Bytes.ofData(a{n})"),
            (_, Type::Handle { .. }) => format!("o{n}"),
            _ => format!("a{n}"),
        })
        .collect();
    let call = format!("{name}({})", passed.join(", "));
    let (succeed, fail) = match returns {
        Type::Void => (format!("{call};\n\t\t\t\treturn true;"), "false"),
        // Null is how the wrapper reports a throw, so it is not a result.
        Type::Str => (
            format!(
                "var result = {call};\n\t\t\t\tif (result == null)\n\t\t\t\t\tthrow new haxe.Exception(\"a function lent to a library returned null\");\n\t\t\t\treturn result;"
            ),
            "null",
        ),
        _ => (format!("return {call};"), "null"),
    };
    format!(
        "var {guarded} = function({declared}):{ty} {{{owned}\n\t\t\ttry {{\n\t\t\t\t{succeed}\n\t\t\t}} catch (error:haxe.Exception) {{\n\t\t\t\thxn.Callback.failed(error);\n\t\t\t\treturn {fail};\n\t\t\t}}\n\t\t}};",
        guarded = guarded(name),
        declared = declared.join(", "),
        ty = haxe_guarded(returns),
    )
}
