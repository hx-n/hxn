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

use crate::model::{Library, Type};

/// What a lent function takes and gives, if `ty` is one.
pub fn of(ty: &Type) -> Option<(&[Type], &Type)> {
    match ty {
        Type::Lent { params, returns } => Some((params, returns)),
        _ => None,
    }
}

/// Each shape of function the library is lent somewhere, in the order met.
pub fn used(library: &Library) -> Vec<&Type> {
    let mut out: Vec<&Type> = Vec::new();
    for method in library.interfaces.iter().flat_map(|i| &i.methods) {
        for param in &method.params {
            if of(&param.ty).is_some() && !out.contains(&&param.ty) {
                out.push(&param.ty);
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
        _ => unreachable!("not passed to or from a lent function"),
    }
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

// What a value is at the boundary: a bool is an int, and a function that
// returns nothing writes an int nobody reads.
fn c_type(ty: &Type) -> &'static str {
    match ty {
        Type::F64 => "f64",
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
            c_type(returns)
        ));
    }
    out
}

/// The closure an adapter passes the library's function for the callback
/// `name`: it calls through, and gives the library an `Err` when the Haxe
/// function failed.
pub fn closure(name: &str, params: &[Type], returns: &Type) -> String {
    let names: Vec<String> = (0..params.len()).map(|n| format!("a{n}")).collect();
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
    let taken: String = params.iter().map(letter).collect();
    let given = match returns {
        // The wrapper's result is whether the function returned.
        Type::Void => String::from("b"),
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
        let boxed: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match ty {
                Type::Bool => format!("box_bool(a{n} != 0)"),
                Type::F64 => format!("box_f64(a{n})"),
                _ => format!("box_i32(a{n})"),
            })
            .collect();
        let (unbox, pattern, written) = match returns {
            Type::Void => ("unbox_bool", "Some(true)", "0"),
            Type::Bool => ("unbox_bool", "Some(result)", "i32::from(result)"),
            Type::F64 => ("unbox_f64", "Some(result)", "result"),
            _ => ("unbox_i32", "Some(result)", "result"),
        };
        out.push_str(&format!(
            "\n// The binding's wrapper gives null, or false, when the Haxe function threw.\nunsafe extern \"C\" fn {name}(env: *mut c_void, {taken}out: *mut {ty}) -> Status {{\n    let mut args: [*mut vdynamic; {count}] = [{boxed}];\n    match unsafe {{ {unbox}(call(env.cast(), &mut args)) }} {{\n        {pattern} => {{\n            unsafe {{ out.write({written}) }};\n            Status::OK\n        }}\n        _ => Status::CALLBACK,\n    }}\n}}\n",
            name = trampoline(params, returns),
            ty = c_type(returns),
            count = params.len(),
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
                _ => format!("int a{n}, "),
            })
            .collect();
        let passed: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(n, ty)| match ty {
                Type::Bool => format!("a{n} != 0"),
                _ => format!("a{n}"),
            })
            .collect();
        let (ty, failed, written) = match returns {
            Type::Void => ("int", "result == null() || !(bool)result", "0"),
            Type::Bool => ("int", "result == null()", "(bool)result ? 1 : 0"),
            Type::F64 => ("double", "result == null()", "(double)result"),
            _ => ("int", "result == null()", "(int)result"),
        };
        out.push_str(&format!(
            "\n// The binding's wrapper gives null, or false, when the Haxe function threw.\ninline hxn_status {name}(void *env, {taken}{ty} *out) {{\n\tDynamic result = Dynamic((hx::Object *)env)({passed});\n\tif ({failed})\n\t\treturn HXN_CALLBACK;\n\t*out = {written};\n\treturn HXN_OK;\n}}\n",
            name = trampoline(params, returns),
            passed = passed.join(", "),
        ));
    }
    out
}

fn haxe_scalar(ty: &Type) -> &'static str {
    match ty {
        Type::Void => "Void",
        Type::Bool => "Bool",
        Type::I32 => "Int",
        Type::F64 => "Float",
        _ => unreachable!("not passed to or from a lent function"),
    }
}

fn haxe_taken(params: &[Type]) -> String {
    let taken: Vec<&str> = params.iter().map(haxe_scalar).collect();
    taken.join(", ")
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
pub fn haxe_native_type(params: &[Type], returns: &Type) -> String {
    format!("({}) -> {}", haxe_taken(params), haxe_guarded(returns))
}

/// The name of the wrapper around the function `name`.
pub fn guarded(name: &str) -> String {
    format!("{name}Guarded")
}

/// The statement that wraps the function `name`: the wrapper catches,
/// keeps the exception and returns what tells the library the call failed.
pub fn haxe_guard(name: &str, params: &[Type], returns: &Type) -> String {
    let declared: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(n, ty)| format!("a{n}:{}", haxe_scalar(ty)))
        .collect();
    let passed: Vec<String> = (0..params.len()).map(|n| format!("a{n}")).collect();
    let call = format!("{name}({})", passed.join(", "));
    let (succeed, fail) = match returns {
        Type::Void => (format!("{call};\n\t\t\t\treturn true;"), "false"),
        _ => (format!("return {call};"), "null"),
    };
    format!(
        "var {guarded} = function({declared}):{ty} {{\n\t\t\ttry {{\n\t\t\t\t{succeed}\n\t\t\t}} catch (error:haxe.Exception) {{\n\t\t\t\thxn.Callback.failed(error);\n\t\t\t\treturn {fail};\n\t\t\t}}\n\t\t}};",
        guarded = guarded(name),
        declared = declared.join(", "),
        ty = haxe_guarded(returns),
    )
}
