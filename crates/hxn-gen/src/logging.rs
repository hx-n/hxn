//! The library-named pull transport for hxn's core diagnostic queue.

use crate::model::Library;
use crate::names::export;

pub fn c_header(library: &Library) -> String {
    format!(
        "\n/* Configures capture and takes a bounded diagnostic batch. */\nvoid {level}(int32_t level);\nhxn_status {take}(hxn_buf *out, hxn_buf *err);\n",
        level = export(library, "Log", "level"),
        take = export(library, "Log", "take")
    )
}

pub fn c_adapter(library: &Library) -> String {
    format!(
        r#"
#[unsafe(no_mangle)]
pub extern "C" fn {level}(level: i32) {{
    hxn_abi::log::capture(level);
}}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn {take}(out: *mut Buf, err: *mut Buf) -> Status {{
    unsafe {{ guard(out, err, || Ok(Buf::from(hxn_abi::log::take_wire()))) }}
}}
"#,
        level = export(library, "Log", "level"),
        take = export(library, "Log", "take")
    )
}

pub fn hl_adapter(library: &Library) -> String {
    format!(
        r#"
#[unsafe(no_mangle)]
pub extern "C" fn {level}(level: i32) {{
    hxn_abi::log::capture(level);
}}
define_prim!(hlp_log_level, {level}, "Pi_v");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn {take}(out: *mut i32) -> *mut vbyte {{
    unsafe {{ prim(|| {{
        if out.is_null() {{ return Err(Fault::Null.into()); }}
        let value = hxn_abi::log::take_wire();
        *out = value.len() as i32;
        Ok(bytes(&value))
    }}) }}
}}
define_prim!(hlp_log_take, {take}, "PRi_B");
"#,
        level = export(library, "Log", "level"),
        take = export(library, "Log", "take")
    )
}

pub fn cpp_glue(library: &Library) -> String {
    format!(
        r#"
inline void log_level(int level) {{ {level}(level); }}
inline ::Array<unsigned char> log_take() {{
    hxn_buf out = HXN_BUF_EMPTY, err = HXN_BUF_EMPTY;
    hxn::check({take}(&out, &err), err);
    if (!out.len) {{ hxn_buf_free(&out); return null(); }}
    return hxn::take_bytes(out);
}}
"#,
        level = export(library, "Log", "level"),
        take = export(library, "Log", "take")
    )
}
