//! A C header to an [`Api`], from the JSON AST clang writes for it.
//!
//! Only declarations in the header itself are imported; what it includes is
//! read to resolve type names. The mapping is a guess at what the header
//! means, written to WebIDL for review:
//!
//! - A struct declared without its fields is an object. A function whose
//!   first parameter points to one is a method of it, named without the
//!   object's prefix: `tally_add` on `tally` is `add`. One named `free`,
//!   `destroy` and so on frees it, and one named `new`, `create` and so on
//!   that returns a pointer to it is its constructor. Another function that
//!   returns one is a static function of it.
//! - A struct with its fields is passed by value, when every field is a
//!   boolean or a number.
//! - Returned `char *` text is freed with the library's one function that
//!   takes a `char *` and frees it, when there is exactly one.
//! - Anything else that cannot be mapped is listed as not imported.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::model::{
    Api, Callback, Enum, Function, Ignored, Int, Object, Param, Role, Status, Struct, Ty, freed,
};

const FREES: [&str; 8] = [
    "free", "destroy", "delete", "release", "dispose", "unref", "close", "finalize",
];
const MAKES: [&str; 6] = ["new", "create", "make", "open", "init", "alloc"];

// Words in the name of a function that keeps the callback it is given, to
// call after it returns. Such a callback is imported as kept, `[HxnKept]`;
// any other as lent for the call.
const KEEPS: [&str; 15] = [
    "set",
    "register",
    "install",
    "hook",
    "handler",
    "listen",
    "listener",
    "subscribe",
    "watch",
    "on",
    "trace",
    "profile",
    "progress",
    "busy",
    "notify",
];

// Whether a parameter named `name` reads as the count of the array before it.
// A pointer followed by an integer is just as often an output and a flag.
fn counts(name: &str) -> bool {
    const WORDS: [&str; 9] = [
        "n", "len", "length", "size", "count", "num", "nelem", "nmemb", "capacity",
    ];
    WORDS.contains(&name)
        || ["_len", "_length", "_size", "_count", "_n"]
            .iter()
            .any(|end| name.ends_with(end))
        || name.starts_with("n_")
        || name.starts_with("num_")
}

fn keeps_callback(function: &str) -> bool {
    function.split('_').any(|word| KEEPS.contains(&word))
}

// A C type as the header spells it.
#[derive(Clone, PartialEq, Debug)]
enum CType {
    Builtin(String),
    Record(String),
    Enum(String),
    Pointer {
        to: Box<CType>,
        constant: bool,
    },
    /// A function pointer: its return type and parameter types as spelled.
    Function {
        returns: String,
        params: Vec<String>,
    },
    Unsupported(String),
}

struct Header {
    // Every typedef clang saw, in the header and in what it includes, and
    // the documentation of those in the header.
    typedefs: HashMap<String, String>,
    typedef_docs: HashMap<String, Vec<String>>,
    // What the header declares.
    records: Vec<Record>,
    enums: Vec<CEnum>,
    functions: Vec<CFunction>,
}

// An enum's tag, its documentation and its constants.
type CEnum = (String, Vec<String>, Vec<(String, i64)>);

struct Record {
    tag: String,
    // The typedef that names it, if any.
    alias: Option<String>,
    docs: Vec<String>,
    fields: Option<Vec<(String, String)>>,
}

struct CFunction {
    name: String,
    docs: Vec<String>,
    returns: String,
    params: Vec<(String, String)>,
    variadic: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn qual_type(node: &Value) -> String {
    node.get("type")
        .and_then(|ty| text(ty, "qualType"))
        .unwrap_or_default()
        .to_owned()
}

// The text of the comment clang attached to a declaration, a line a
// paragraph line.
fn comment(node: &Value) -> Vec<String> {
    fn gather(node: &Value, out: &mut String) {
        if text(node, "kind") == Some("TextComment")
            && let Some(line) = text(node, "text")
        {
            // Many headers start each comment line with `*` or `**`.
            out.push_str(line.trim().trim_start_matches('*').trim());
            out.push(' ');
        }
        if text(node, "kind") == Some("ParagraphComment") && !out.is_empty() {
            out.push('\n');
        }
        for child in node
            .get("inner")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            gather(child, out);
        }
    }
    let Some(full) = node
        .get("inner")
        .and_then(Value::as_array)
        .and_then(|inner| {
            inner
                .iter()
                .find(|n| text(n, "kind") == Some("FullComment"))
        })
    else {
        return Vec::new();
    };
    let mut out = String::new();
    gather(full, &mut out);
    let joined: Vec<String> = out
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    // Wrapped at about 76 columns, as comments in the WebIDL.
    let mut lines = Vec::new();
    for paragraph in joined {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            if !line.is_empty() && line.len() + word.len() > 74 {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    lines
}

// The file a node is declared in. clang writes a location's file only when it
// differs from the location written before it, so `current` carries the last
// one seen.
fn file_of(node: &Value, current: &mut Option<String>) {
    let loc = node.get("loc");
    let candidates = [
        loc.and_then(|l| text(l, "file")),
        loc.and_then(|l| l.get("spellingLoc"))
            .and_then(|l| text(l, "file")),
        loc.and_then(|l| l.get("expansionLoc"))
            .and_then(|l| text(l, "file")),
        node.get("range")
            .and_then(|r| r.get("begin"))
            .and_then(|b| text(b, "file")),
    ];
    if let Some(file) = candidates.into_iter().flatten().next() {
        *current = Some(file.to_owned());
    }
}

fn read(ast: &Value, header: &str) -> Header {
    let mut out = Header {
        typedefs: HashMap::new(),
        typedef_docs: HashMap::new(),
        records: Vec::new(),
        enums: Vec::new(),
        functions: Vec::new(),
    };
    let mut current: Option<String> = None;
    let canonical = |path: &str| std::fs::canonicalize(path).ok();
    let wanted = canonical(header);
    let mut known_files: HashMap<String, bool> = HashMap::new();
    for node in ast
        .get("inner")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        file_of(node, &mut current);
        let kind = text(node, "kind").unwrap_or_default();
        let name = text(node, "name").unwrap_or_default().to_owned();
        if kind == "TypedefDecl" {
            out.typedefs.insert(name.clone(), qual_type(node));
        }
        let ours = current.as_ref().is_some_and(|file| {
            *known_files
                .entry(file.clone())
                .or_insert_with(|| canonical(file).is_some_and(|path| Some(path) == wanted))
        });
        if !ours || text(node, "isImplicit").is_some() {
            continue;
        }
        let inner = node.get("inner").and_then(Value::as_array);
        match kind {
            "RecordDecl" if text(node, "tagUsed") == Some("struct") && !name.is_empty() => {
                let complete =
                    node.get("completeDefinition").and_then(Value::as_bool) == Some(true);
                let fields = complete.then(|| {
                    inner
                        .into_iter()
                        .flatten()
                        .filter(|n| text(n, "kind") == Some("FieldDecl"))
                        .map(|n| (text(n, "name").unwrap_or_default().to_owned(), qual_type(n)))
                        .collect()
                });
                match out.records.iter_mut().find(|record| record.tag == name) {
                    Some(record) => {
                        if fields.is_some() {
                            record.fields = fields;
                        }
                        if record.docs.is_empty() {
                            record.docs = comment(node);
                        }
                    }
                    None => out.records.push(Record {
                        tag: name,
                        alias: None,
                        docs: comment(node),
                        fields,
                    }),
                }
            }
            "EnumDecl" if !name.is_empty() => {
                let mut next = 0;
                let mut values = Vec::new();
                for constant in inner.into_iter().flatten() {
                    if text(constant, "kind") != Some("EnumConstantDecl") {
                        continue;
                    }
                    // clang records the value only where it was written.
                    let written = constant
                        .get("inner")
                        .and_then(Value::as_array)
                        .and_then(|inner| inner.first())
                        .and_then(|expr| text(expr, "value"))
                        .and_then(|value| value.parse::<i64>().ok());
                    let value = written.unwrap_or(next);
                    next = value + 1;
                    values.push((text(constant, "name").unwrap_or_default().to_owned(), value));
                }
                out.enums.push((name, comment(node), values));
            }
            "TypedefDecl" => {
                let target = qual_type(node);
                let docs = comment(node);
                if !docs.is_empty() {
                    out.typedef_docs.insert(name.clone(), docs);
                }
                if let Some(tag) = target.strip_prefix("struct ")
                    && let Some(record) = out.records.iter_mut().find(|r| r.tag == tag)
                {
                    record.alias.get_or_insert(name);
                }
            }
            "FunctionDecl" if text(node, "storageClass") != Some("static") => {
                let signature = qual_type(node);
                let params: Vec<(String, String)> = inner
                    .into_iter()
                    .flatten()
                    .filter(|n| text(n, "kind") == Some("ParmVarDecl"))
                    .map(|n| (text(n, "name").unwrap_or_default().to_owned(), qual_type(n)))
                    .collect();
                // The return type is what comes before the parameter list.
                let returns = signature
                    .find('(')
                    .map_or(signature.as_str(), |at| &signature[..at])
                    .trim()
                    .to_owned();
                out.functions.push(CFunction {
                    name,
                    docs: comment(node),
                    returns,
                    params,
                    variadic: node.get("variadic").and_then(Value::as_bool) == Some(true),
                });
            }
            _ => {}
        }
    }
    out
}

const BUILTINS: [&str; 22] = [
    "void",
    "_Bool",
    "bool",
    "char",
    "signed char",
    "unsigned char",
    "short",
    "unsigned short",
    "int",
    "unsigned int",
    "long",
    "unsigned long",
    "long long",
    "unsigned long long",
    "float",
    "double",
    "size_t",
    "ssize_t",
    "ptrdiff_t",
    "int8_t",
    "uint8_t",
    "intptr_t",
];

const FIXED: [&str; 6] = [
    "int16_t", "uint16_t", "int32_t", "uint32_t", "int64_t", "uint64_t",
];

impl Header {
    // `spelled` as a type, typedefs resolved down to a builtin, a record or
    // an enum. The fixed-width and size typedefs are kept as they are.
    fn c_type(&self, spelled: &str) -> CType {
        let spelled = spelled.trim();
        // `int (*)(void *, int32_t)`.
        if let Some(at) = spelled.find("(*)") {
            let returns = spelled[..at].trim().to_owned();
            let rest = spelled[at + 3..].trim();
            let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
                return CType::Unsupported(spelled.to_owned());
            };
            let mut params = Vec::new();
            let mut depth = 0;
            let mut current = String::new();
            for letter in inner.chars() {
                match letter {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ',' if depth == 0 => {
                        params.push(std::mem::take(&mut current).trim().to_owned());
                        continue;
                    }
                    _ => {}
                }
                current.push(letter);
            }
            if !current.trim().is_empty() {
                params.push(current.trim().to_owned());
            }
            if params == ["void"] {
                params.clear();
            }
            return CType::Function { returns, params };
        }
        if spelled.contains('(') || spelled.contains('[') {
            return CType::Unsupported(spelled.to_owned());
        }
        if let Some(at) = spelled.rfind('*') {
            let inner = spelled[..at].trim();
            // `const` qualifies what it is written beside: in `const char **`
            // the pointee is a `const char *`, which is not itself const.
            let constant = if inner.contains('*') {
                inner.ends_with("const")
            } else {
                inner.starts_with("const ") || inner.ends_with(" const")
            };
            return CType::Pointer {
                to: Box::new(self.c_type(inner)),
                constant,
            };
        }
        let base = spelled
            .split_whitespace()
            .filter(|word| !["const", "volatile", "restrict"].contains(word))
            .collect::<Vec<_>>()
            .join(" ");
        let base = match base.as_str() {
            "unsigned" | "unsigned int" => String::from("unsigned int"),
            "short int" | "signed short" | "signed short int" => String::from("short"),
            "unsigned short int" => String::from("unsigned short"),
            "long int" | "signed long" => String::from("long"),
            "unsigned long int" => String::from("unsigned long"),
            "long long int" | "signed long long" => String::from("long long"),
            "unsigned long long int" => String::from("unsigned long long"),
            "signed" | "signed int" => String::from("int"),
            other => other.to_owned(),
        };
        if BUILTINS.contains(&base.as_str()) || FIXED.contains(&base.as_str()) {
            return CType::Builtin(base);
        }
        if let Some(tag) = base.strip_prefix("struct ") {
            return CType::Record(tag.to_owned());
        }
        if let Some(tag) = base.strip_prefix("enum ") {
            return CType::Enum(tag.to_owned());
        }
        match self.typedefs.get(&base) {
            Some(target) if *target != base => self.c_type(target),
            _ => CType::Unsupported(base),
        }
    }
}

// A C name in snake case, as Rust names parameters and functions:
// `zDbName` to `z_db_name`, `HTTPServer` to `http_server`.
fn snake(name: &str) -> String {
    let letters: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (at, letter) in letters.iter().enumerate() {
        if letter.is_ascii_uppercase() {
            let before = at.checked_sub(1).map(|n| letters[n]);
            let after = letters.get(at + 1);
            let starts_word = before.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                || (before.is_some_and(|b| b.is_ascii_uppercase())
                    && after.is_some_and(char::is_ascii_lowercase));
            if starts_word && !out.ends_with('_') {
                out.push('_');
            }
            out.push(letter.to_ascii_lowercase());
        } else {
            out.push(*letter);
        }
    }
    out
}

// `tally_mode_t` to `Mode` for library `tally`.
fn type_name(library: &str, c: &str) -> String {
    let c = c.strip_suffix("_t").unwrap_or(c);
    let stem = if c == library {
        c
    } else {
        c.strip_prefix(&format!("{library}_")).unwrap_or(c)
    };
    crate::idl::namespace(stem)
}

// `TALLY_MODE_SUM` to `Sum`, given every constant of the enum.
fn value_names(constants: &[String]) -> Vec<String> {
    let mut prefix = constants.first().cloned().unwrap_or_default();
    for constant in constants {
        while !constant.starts_with(&prefix) {
            prefix.pop();
        }
    }
    // Cut at an underscore, so `MODE_MAX` and `MODE_MIN` keep their `M`.
    let cut = prefix.rfind('_').map_or(0, |at| at + 1);
    constants
        .iter()
        .map(|constant| {
            let rest = &constant[cut..];
            let rest = if rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit()) {
                constant.as_str()
            } else {
                rest
            };
            crate::idl::namespace(&rest.to_lowercase())
        })
        .collect()
}

struct Context<'a> {
    library: &'a str,
    header: &'a Header,
    // C tag of each object, and its name.
    objects: Vec<(String, String)>,
    structs: Vec<(String, String)>,
    enums: Vec<(String, String)>,
    // The function pointer types met so far, and a note for each function
    // that is lent or keeps one.
    callbacks: RefCell<Vec<Callback>>,
    lent: RefCell<Vec<String>>,
    // The callbacks a function keeps.
    kept: RefCell<Vec<String>>,
}

impl Context<'_> {
    // The element and count types of a pointer to `to` followed by `next`,
    // if they are an array of numbers and its count.
    fn array(&self, to: &CType, next: Option<&(String, String)>) -> Option<(Ty, Int)> {
        let element = self.value(to).ok()?;
        let fits =
            matches!(&element, Ty::Int(int) if int.fixed()) || matches!(element, Ty::F32 | Ty::F64);
        let (name, spelled) = next?;
        let Ok(Ty::Int(length)) = self.value(&self.header.c_type(spelled)) else {
            return None;
        };
        (fits && counts(&snake(name))).then_some((element, length))
    }

    // The callback a parameter spelled `spelled` is, made the first time it
    // is met. `fallback` names it when no typedef does. `kept` is whether
    // the function it is first met in keeps it, which sets what C is
    // returned: 0 or false by default for a kept callback, which is called
    // on every event, and 1 or false for a lent one when the Haxe function
    // throws.
    fn callback(&self, spelled: &str, fallback: &str, kept: bool) -> Result<String, String> {
        let CType::Function { returns, params } = self.header.c_type(spelled) else {
            return Err(format!("`{spelled}` is not a function pointer"));
        };
        let typedef = spelled.trim_start_matches("const ").trim();
        let named = self.header.typedefs.contains_key(typedef);
        let name = if named {
            type_name(self.library, typedef)
        } else {
            crate::idl::namespace(fallback)
        };
        if self
            .callbacks
            .borrow()
            .iter()
            .any(|known| known.name == name)
        {
            return Ok(name);
        }
        let void_pointer = CType::Pointer {
            to: Box::new(CType::Builtin(String::from("void"))),
            constant: false,
        };
        let user: Vec<usize> = params
            .iter()
            .enumerate()
            .filter(|(_, p)| self.header.c_type(p) == void_pointer)
            .map(|(at, _)| at)
            .collect();
        let user_first = match user.as_slice() {
            [0] => true,
            [at] if *at + 1 == params.len() => false,
            _ => {
                return Err(format!(
                    "`{spelled}` takes its `void *` user data neither first nor last"
                ));
            }
        };
        let rest: Vec<&String> = params
            .iter()
            .enumerate()
            .filter(|(at, _)| !user.contains(at))
            .map(|(_, p)| p)
            .collect();
        let mut mapped = Vec::new();
        for (at, param) in rest.iter().enumerate() {
            let name = if rest.len() == 1 {
                String::from("value")
            } else {
                format!("value{at}")
            };
            let ty = match self.header.c_type(param) {
                CType::Pointer { to, .. } if matches!(&*to, CType::Builtin(b) if b == "char" || b == "unsigned char") => {
                    Ty::Text
                }
                other => match self.value(&other) {
                    Ok(
                        ty @ (Ty::Bool
                        | Ty::Int(_)
                        | Ty::F32
                        | Ty::F64
                        | Ty::Enum(_)
                        | Ty::Struct(_)),
                    ) => ty,
                    _ => {
                        return Err(format!(
                            "`{spelled}` takes `{param}`, which a Haxe function is not passed"
                        ));
                    }
                },
            };
            mapped.push(Param::new(name, ty));
        }
        let (returns, stop) = match self.value(&self.header.c_type(&returns)) {
            Ok(Ty::Void) => (Ty::Void, None),
            Ok(Ty::Bool) => (Ty::Bool, Some(0)),
            Ok(Ty::Int(Int::I32)) => (Ty::Int(Int::I32), Some(i64::from(!kept))),
            _ => {
                return Err(format!(
                    "`{spelled}` returns `{returns}`; a callback returns nothing, a bool or an int"
                ));
            }
        };
        self.callbacks.borrow_mut().push(Callback {
            name: name.clone(),
            docs: self
                .header
                .typedef_docs
                .get(typedef)
                .cloned()
                .unwrap_or_default(),
            params: mapped,
            returns,
            user_first,
            stop,
        });
        Ok(name)
    }

    fn object(&self, ty: &CType) -> Option<String> {
        let CType::Pointer { to, .. } = ty else {
            return None;
        };
        let CType::Record(tag) = &**to else {
            return None;
        };
        self.objects
            .iter()
            .find(|(c, _)| c == tag)
            .map(|(_, name)| name.clone())
    }

    fn named<'b>(list: &'b [(String, String)], tag: &str) -> Option<&'b String> {
        list.iter().find(|(c, _)| c == tag).map(|(_, name)| name)
    }

    // A value of a builtin, enum or struct type.
    fn value(&self, ty: &CType) -> Result<Ty, String> {
        match ty {
            CType::Builtin(name) => Ok(match name.as_str() {
                "void" => Ty::Void,
                "_Bool" | "bool" => Ty::Bool,
                "char" | "signed char" | "int8_t" => Ty::Int(Int::I8),
                "unsigned char" | "uint8_t" => Ty::Int(Int::U8),
                "short" | "int16_t" => Ty::Int(Int::I16),
                "unsigned short" | "uint16_t" => Ty::Int(Int::U16),
                "int" | "int32_t" => Ty::Int(Int::I32),
                "unsigned int" | "uint32_t" => Ty::Int(Int::U32),
                "long long" | "int64_t" => Ty::Int(Int::I64),
                "unsigned long long" | "uint64_t" => Ty::Int(Int::U64),
                "long" => Ty::Int(Int::Long),
                "unsigned long" => Ty::Int(Int::ULong),
                "size_t" => Ty::Int(Int::Size),
                "ssize_t" | "ptrdiff_t" | "intptr_t" => Ty::Int(Int::SSize),
                "float" => Ty::F32,
                "double" => Ty::F64,
                other => return Err(format!("`{other}` has no mapping")),
            }),
            CType::Enum(tag) => Self::named(&self.enums, tag)
                .map(|name| Ty::Enum(name.clone()))
                .ok_or_else(|| format!("`enum {tag}` is not declared in the header")),
            CType::Record(tag) => Self::named(&self.structs, tag)
                .map(|name| Ty::Struct(name.clone()))
                .ok_or_else(|| format!("`struct {tag}` is not passed by value")),
            CType::Pointer { .. } => Err(String::from("pointer")),
            CType::Function { .. } => Err(String::from("a function pointer")),
            CType::Unsupported(spelled) => Err(format!("`{spelled}` has no mapping")),
        }
    }

    // The parameters, with a byte pointer and the `size_t` after it read as
    // one `Uint8Array`.
    fn params(&self, function: &str, given: &[(String, String)]) -> Result<Vec<Param>, String> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < given.len() {
            let (name, spelled) = &given[at];
            let name = if name.is_empty() {
                format!("arg{at}")
            } else {
                snake(name)
            };
            let ty = self.header.c_type(spelled);
            // A function pointer and the `void *` user data after it.
            if matches!(ty, CType::Function { .. }) {
                let user = given.get(at + 1).map(|(_, s)| self.header.c_type(s));
                let void_pointer = CType::Pointer {
                    to: Box::new(CType::Builtin(String::from("void"))),
                    constant: false,
                };
                // A destructor of the text or bytes before it.
                let destructor = match &ty {
                    CType::Function { returns, params } => {
                        self.header.c_type(returns) == CType::Builtin(String::from("void"))
                            && params.len() == 1
                            && self.header.c_type(&params[0]) == void_pointer
                    }
                    _ => false,
                };
                if user != Some(void_pointer.clone()) && destructor {
                    out.push(Param {
                        name,
                        ty: Ty::Pointer,
                        role: Role::Frees,
                    });
                    let frees = &out[out.len() - 1].name;
                    match freed(&out, out.len() - 1) {
                        Some(data) => self.lent.borrow_mut().push(format!(
                            "{function}: `{frees}` is taken to free `{}` ([HxnFrees]): C is given a copy, which it frees by calling it",
                            out[data].name
                        )),
                        None => {
                            return Err(format!(
                                "`{frees}`: a destructor with no text or bytes before it to free"
                            ));
                        }
                    }
                    at += 1;
                    continue;
                }
                if user != Some(void_pointer) {
                    return Err(format!(
                        "`{name}`: a function pointer is passed with its `void *` user data after it. If C is always passed the same value for it, such as a destructor constant, add the function by hand with `[HxnPass=<value>] any {name}`"
                    ));
                }
                let kept = keeps_callback(function);
                let stem = function
                    .strip_prefix(&format!("{}_", self.library))
                    .unwrap_or(function);
                let callback = self
                    .callback(spelled, &format!("{stem}_{name}"), kept)
                    .map_err(|why| format!("`{name}`: {why}"))?;
                if kept {
                    self.lent.borrow_mut().push(format!(
                        "{function}: C is taken to keep `{name}` ([HxnKept]), and the binding returns a stream of what C calls it with"
                    ));
                    let mut known = self.kept.borrow_mut();
                    if !known.contains(&callback) {
                        known.push(callback.clone());
                    }
                } else {
                    let callbacks = self.callbacks.borrow();
                    let Some(made) = callbacks.iter().find(|known| known.name == callback) else {
                        unreachable!("a callback just made")
                    };
                    if made.params.len() > 4 {
                        return Err(format!(
                            "`{name}`: takes {} values, and a Haxe function lent to C takes at most 4",
                            made.params.len()
                        ));
                    }
                    if made
                        .params
                        .iter()
                        .any(|p| matches!(p.ty, Ty::Int(int) if int != Int::I32))
                    {
                        return Err(format!(
                            "`{name}`: a Haxe function lent to C takes integers as C `int`"
                        ));
                    }
                    self.lent.borrow_mut().push(format!(
                        "{function}: C must call `{name}` only before {function} returns; mark it [HxnKept] if C keeps it"
                    ));
                }
                out.push(Param {
                    name,
                    ty: Ty::Callback(callback),
                    role: if kept { Role::Kept } else { Role::In },
                });
                at += 2;
                continue;
            }
            let param = match &ty {
                CType::Pointer { to, constant: true } => match &**to {
                    CType::Builtin(base) if base == "char" => Ty::Text,
                    CType::Builtin(base)
                        if ["unsigned char", "uint8_t", "void"].contains(&base.as_str()) =>
                    {
                        let length = given
                            .get(at + 1)
                            .map(|(_, s)| self.value(&self.header.c_type(s)));
                        let Some(Ok(Ty::Int(length))) = length else {
                            return Err(format!(
                                "`{name}`: a byte pointer is passed with its length, an integer, after it"
                            ));
                        };
                        at += 1;
                        Ty::Bytes(length)
                    }
                    other if self.array(other, given.get(at + 1)).is_some() => {
                        let (element, length) =
                            self.array(other, given.get(at + 1)).expect("matched");
                        at += 1;
                        Ty::Array(Box::new(element), length)
                    }
                    CType::Record(tag) => match Self::named(&self.structs, tag) {
                        Some(record) => Ty::StructRef(record.clone()),
                        None => match self.object(&ty) {
                            Some(object) => Ty::Object(object),
                            None => return Err(format!("`{name}`: `{spelled}` has no mapping")),
                        },
                    },
                    _ => return Err(format!("`{name}`: `{spelled}` has no mapping")),
                },
                // Bytes C writes into, with their length after them. The length
                // must read as one: a buffer C keeps, such as the one
                // `sqlite3_deserialize` takes, is passed with a size that does not.
                CType::Pointer { to, .. }
                    if matches!(&**to, CType::Builtin(base) if ["char", "signed char", "unsigned char", "uint8_t", "int8_t", "void"].contains(&base.as_str()))
                        && given
                            .get(at + 1)
                            .is_some_and(|(length, _)| counts(&snake(length)))
                        && matches!(
                            given
                                .get(at + 1)
                                .map(|(_, s)| self.value(&self.header.c_type(s))),
                            Some(Ok(Ty::Int(_)))
                        ) =>
                {
                    let Some(Ok(Ty::Int(length))) = given
                        .get(at + 1)
                        .map(|(_, s)| self.value(&self.header.c_type(s)))
                    else {
                        unreachable!("matched above")
                    };
                    self.lent.borrow_mut().push(format!(
                        "{function}: C is taken to write into `{name}` during the call ([HxnWritten]); remove the function if C keeps the pointer"
                    ));
                    out.push(Param {
                        name,
                        ty: Ty::Bytes(length),
                        role: Role::Written,
                    });
                    at += 2;
                    continue;
                }
                // Numbers C fills, with their count after them.
                CType::Pointer { to, .. } if self.array(to, given.get(at + 1)).is_some() => {
                    let (element, length) = self.array(to, given.get(at + 1)).expect("matched");
                    self.lent.borrow_mut().push(format!(
                        "{function}: C is taken to fill `{name}`, as many as `{}` gives ([HxnOut]); the caller passes the count and gets the array",
                        snake(&given[at + 1].0)
                    ));
                    out.push(Param {
                        name,
                        ty: Ty::Array(Box::new(element), length),
                        role: Role::Filled,
                    });
                    at += 2;
                    continue;
                }
                CType::Pointer { to, .. } => match self.object(&ty) {
                    Some(object) => Ty::Object(object),
                    None => match self.written(to) {
                        // Taken for an output; `finish` checks where it is.
                        Some(written) => {
                            out.push(Param {
                                name,
                                ty: written,
                                role: Role::Out,
                            });
                            at += 1;
                            continue;
                        }
                        None => {
                            return Err(format!(
                                "`{name}`: `{spelled}` may be an array, which is not imported"
                            ));
                        }
                    },
                },
                other => self
                    .value(other)
                    .map_err(|why| format!("`{name}`: {why}"))?,
            };
            if param == Ty::Void {
                return Err(format!("`{name}` is `void`"));
            }
            out.push(Param::new(name, param));
            at += 1;
        }
        Ok(out)
    }

    // What C writes through a pointer to `to`, if it can be an output: a
    // number, an enum, a struct, an object pointer or a text pointer. A
    // pointer to bytes or characters is a buffer, which is not.
    fn written(&self, to: &CType) -> Option<Ty> {
        match to {
            CType::Builtin(base)
                if [
                    "char",
                    "signed char",
                    "unsigned char",
                    "uint8_t",
                    "int8_t",
                    "void",
                ]
                .contains(&base.as_str()) =>
            {
                None
            }
            CType::Builtin(_) | CType::Enum(_) | CType::Record(_) => self.value(to).ok(),
            CType::Pointer { to: inner, .. } => match &**inner {
                CType::Builtin(base) if base == "char" => Some(Ty::Text),
                _ => self.object(to).map(Ty::Object),
            },
            CType::Function { .. } | CType::Unsupported(_) => None,
        }
    }

    // A function's parameters and C return type as what it returns: an
    // `[HxnOut]` parameter, which comes after every parameter that is passed,
    // makes the return value a status or nothing. Outputs after the first
    // are passed null. Guesses are added to `review`.
    fn finish(
        &self,
        function: &str,
        mut params: Vec<Param>,
        c_returns: &str,
        review: &mut Vec<String>,
    ) -> Result<(Vec<Param>, Ty, Status, Ignored), String> {
        let kept: Vec<&Param> = params.iter().filter(|p| p.role == Role::Kept).collect();
        if let Some(param) = kept.first() {
            if kept.len() > 1 || params.iter().any(|p| p.role == Role::Out) {
                return Err(String::from(
                    "keeps a callback and returns more than the stream of its calls, which is not imported",
                ));
            }
            // What a registration function returns: nothing, a status, or
            // the callback or user data it replaced.
            let (status, ignored) = match self.header.c_type(c_returns) {
                CType::Builtin(base) if base == "void" => (Status::None, Ignored::None),
                CType::Builtin(base) if base == "int" => {
                    review.push(format!(
                        "{function}: its int return value is taken as a status ([HxnStatus]); write [HxnIgnore=int] instead if it is not one"
                    ));
                    (Status::Zero, Ignored::None)
                }
                CType::Pointer { .. } => {
                    review.push(format!(
                        "{function}: the pointer it returns is ignored ([HxnIgnore=pointer]): the binding does not give back what `{}` replaced",
                        param.name
                    ));
                    (Status::None, Ignored::Pointer)
                }
                _ => {
                    return Err(format!(
                        "keeps a callback and returns `{c_returns}`, which is not imported"
                    ));
                }
            };
            return Ok((params, Ty::Void, status, ignored));
        }
        if let Some(filled) = params.iter().find(|p| p.role == Role::Filled) {
            if params
                .iter()
                .filter(|p| matches!(p.role, Role::Out | Role::Filled))
                .count()
                > 1
            {
                return Err(String::from(
                    "fills an array and writes another output, which is not imported",
                ));
            }
            let returns = filled.ty.clone();
            let status = match self.header.c_type(c_returns) {
                CType::Builtin(base) if base == "void" => Status::None,
                CType::Builtin(base) if base == "int" => {
                    review.push(format!(
                        "{function}: its int return value is taken as a status ([HxnStatus])"
                    ));
                    Status::Zero
                }
                _ => {
                    return Err(format!(
                        "fills `{}` and returns `{c_returns}`, which is not imported",
                        filled.name
                    ));
                }
            };
            return Ok((params, returns, status, Ignored::None));
        }
        let Some(first) = params.iter().position(|p| p.role == Role::Out) else {
            let returns = self.returns(c_returns)?;
            return Ok((params, returns, Status::None, Ignored::None));
        };
        if let Some(late) = params[first..].iter().find(|p| p.role.passed()) {
            return Err(format!(
                "`{}` comes before `{}`, so it may be an array, which is not imported",
                params[first].name, late.name
            ));
        }
        for param in &mut params[first + 1..] {
            param.role = Role::Null;
        }
        let out = params[first].clone();
        let status = match self.header.c_type(c_returns) {
            CType::Builtin(base) if base == "void" => Status::None,
            CType::Builtin(base) if base == "int" => Status::Zero,
            _ => {
                return Err(format!(
                    "returns `{c_returns}` and writes `{}` through a pointer, which is not imported",
                    out.name
                ));
            }
        };
        let mut guess = format!(
            "{function}: `{}` is taken as an output ([HxnOut])",
            out.name
        );
        if status == Status::Zero {
            guess.push_str(", and its int return value as a status ([HxnStatus])");
        }
        if params.len() > first + 1 {
            let nulled: Vec<&str> = params[first + 1..]
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            guess.push_str(&format!(
                ", passing null for `{}` ([HxnNull])",
                nulled.join("`, `")
            ));
        }
        review.push(guess);
        let returns = match out.ty {
            Ty::Text => Ty::Nullable(Box::new(Ty::Text)),
            other => other,
        };
        Ok((params, returns, status, Ignored::None))
    }

    fn returns(&self, spelled: &str) -> Result<Ty, String> {
        let ty = self.header.c_type(spelled);
        match &ty {
            // `const unsigned char *` is UTF-8 in many libraries, sqlite3's
            // among them.
            CType::Pointer { to, .. } if matches!(&**to, CType::Builtin(b) if b == "char" || b == "unsigned char") => {
                Ok(Ty::Nullable(Box::new(Ty::Text)))
            }
            CType::Pointer { .. } => match self.object(&ty) {
                Some(object) => Ok(Ty::Nullable(Box::new(Ty::Object(object)))),
                None => Err(format!("returns `{spelled}`, which has no mapping")),
            },
            other => self.value(other).map_err(|why| format!("returns {why}")),
        }
    }
}

/// Imports the C header at `header` as the API of library `library`.
/// `arguments` are passed to clang, such as `-I` directories.
pub fn import(header: &Path, library: &str, arguments: &[String]) -> Result<Api, String> {
    let output = Command::new("clang")
        .args([
            "-Xclang",
            "-ast-dump=json",
            "-fsyntax-only",
            "-fparse-all-comments",
            "-x",
            "c",
        ])
        .args(arguments)
        .arg(header)
        .output()
        .map_err(|error| {
            format!("clang could not be run ({error}): install clang, which reads the header")
        })?;
    if !output.status.success() {
        return Err(format!(
            "clang could not read {}:\n{}",
            header.display(),
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    let ast: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("clang's AST is not JSON: {error}"))?;
    let read = read(&ast, &header.to_string_lossy());
    Ok(api_of(library, &read))
}

fn api_of(library: &str, header: &Header) -> Api {
    let mut api = Api {
        library: library.to_owned(),
        ..Api::default()
    };
    let mut context = Context {
        library,
        header,
        objects: Vec::new(),
        structs: Vec::new(),
        enums: Vec::new(),
        callbacks: RefCell::new(Vec::new()),
        lent: RefCell::new(Vec::new()),
        kept: RefCell::new(Vec::new()),
    };
    for (tag, docs, values) in &header.enums {
        let constants: Vec<String> = values.iter().map(|(name, _)| name.clone()).collect();
        let name = type_name(library, tag);
        api.enums.push(Enum {
            name: name.clone(),
            docs: docs.clone(),
            values: value_names(&constants)
                .into_iter()
                .zip(values.iter().map(|(_, value)| *value))
                .collect(),
        });
        context.enums.push((tag.clone(), name));
    }
    for record in &header.records {
        let c = record.alias.as_deref().unwrap_or(&record.tag);
        let name = type_name(library, c);
        match &record.fields {
            None => {
                context.objects.push((record.tag.clone(), name.clone()));
                api.objects.push(Object {
                    name,
                    docs: record.docs.clone(),
                    free: None,
                    free_int: false,
                    thread_safe: false,
                    constructor: None,
                    methods: Vec::new(),
                    statics: Vec::new(),
                });
            }
            Some(fields) => {
                let mut mapped = Vec::new();
                for (field, spelled) in fields {
                    match context.value(&header.c_type(spelled)) {
                        Ok(ty @ (Ty::Bool | Ty::Int(_) | Ty::F32 | Ty::F64)) => {
                            mapped.push(Param::new(snake(field), ty));
                        }
                        _ => {
                            api.skipped.push(format!(
                                "struct {}: field `{field}` is `{spelled}`; a struct passed by value holds booleans and numbers",
                                record.tag
                            ));
                            mapped.clear();
                            break;
                        }
                    }
                }
                if !mapped.is_empty() {
                    context.structs.push((record.tag.clone(), name.clone()));
                    api.structs.push(Struct {
                        name,
                        docs: record.docs.clone(),
                        fields: mapped,
                    });
                }
            }
        }
    }

    // The one function that frees returned text, if there is one.
    let text_frees: Vec<&CFunction> = header
        .functions
        .iter()
        .filter(|f| {
            f.returns == "void"
                && f.params.len() == 1
                && header.c_type(&f.params[0].1)
                    == (CType::Pointer {
                        to: Box::new(CType::Builtin(String::from("char"))),
                        constant: false,
                    })
                && FREES.iter().any(|word| f.name.ends_with(word))
        })
        .collect();
    let text_free = match text_frees.as_slice() {
        [only] => Some(only.name.clone()),
        _ => None,
    };

    for function in &header.functions {
        if Some(&function.name) == text_free.as_ref() {
            continue;
        }
        if function.variadic {
            api.skipped.push(format!(
                "{}: takes a variable number of arguments",
                function.name
            ));
            continue;
        }
        // A function whose first parameter is an object is its method.
        let owner = function
            .params
            .first()
            .and_then(|(_, spelled)| context.object(&header.c_type(spelled)));
        let returned = context.object(&header.c_type(&function.returns));
        let object_of = |name: &str| {
            context
                .objects
                .iter()
                .find(|(_, n)| n == name)
                .map(|(tag, _)| tag.clone())
        };
        let prefix = |object: &str| {
            let tag = object_of(object).unwrap_or_default();
            let alias = header
                .records
                .iter()
                .find(|r| r.tag == tag)
                .and_then(|r| r.alias.clone())
                .unwrap_or(tag);
            alias.strip_suffix("_t").unwrap_or(&alias).to_owned()
        };
        // Without the object's prefix, or else the library's.
        let short = |stem: &str| {
            let name = &function.name;
            snake(
                name.strip_prefix(&format!("{stem}_"))
                    .or_else(|| name.strip_prefix(&format!("{library}_")))
                    .unwrap_or(name),
            )
        };
        // Text returned through a pointer that is not `const` is the caller's
        // to free.
        let text = matches!(
            header.c_type(&function.returns),
            CType::Pointer { to, constant: false }
                if matches!(&*to, CType::Builtin(b) if b == "char" || b == "unsigned char")
        );
        let free = text.then(|| text_free.clone()).flatten();

        if let Some(object) = &owner {
            let name = short(&prefix(object));
            let index = api
                .objects
                .iter()
                .position(|o| &o.name == object)
                .unwrap_or(0);
            let returns = header.c_type(&function.returns);
            let int = returns == CType::Builtin(String::from("int"));
            if function.params.len() == 1
                && (int || returns == CType::Builtin(String::from("void")))
                && FREES.contains(&name.as_str())
                && api.objects[index].free.is_none()
            {
                api.objects[index].free = Some(function.name.clone());
                api.objects[index].free_int = int;
                continue;
            }
            // An object a method returns usually belongs to its receiver:
            // `sqlite3_db_handle` gives the statement's database.
            let borrowed = context.object(&header.c_type(&function.returns)).is_some();
            if borrowed {
                api.review.push(format!(
                    "{}: the object it returns is taken as borrowed ([HxnBorrowed]), not freed by Haxe",
                    function.name
                ));
            }
            match context
                .params(&function.name, &function.params[1..])
                .and_then(|params| {
                    context.finish(&function.name, params, &function.returns, &mut api.review)
                }) {
                Ok((params, returns, status, ignored)) => {
                    api.objects[index].methods.push(Function {
                        name,
                        c: function.name.clone(),
                        docs: function.docs.clone(),
                        params,
                        returns,
                        status,
                        free,
                        borrowed,
                        ignored,
                    });
                }
                Err(why) => api.skipped.push(format!("{}: {why}", function.name)),
            }
            continue;
        }
        let (params, returns, status, ignored) = match context
            .params(&function.name, &function.params)
            .and_then(|params| {
                context.finish(&function.name, params, &function.returns, &mut api.review)
            }) {
            Ok(finished) => finished,
            Err(why) => {
                api.skipped.push(format!("{}: {why}", function.name));
                continue;
            }
        };
        // An object it returns, or writes through a pointer, makes it the
        // object's constructor or one of its static functions.
        let made_object = match (&returned, &returns) {
            (Some(object), _) => Some(object.clone()),
            (None, Ty::Object(object)) => Some(object.clone()),
            _ => None,
        };
        if let Some(object) = &made_object {
            let name = short(&prefix(object));
            let index = api
                .objects
                .iter()
                .position(|o| &o.name == object)
                .unwrap_or(0);
            let made = Function {
                name: name.clone(),
                c: function.name.clone(),
                docs: function.docs.clone(),
                params,
                returns,
                status,
                free: None,
                borrowed: false,
                ignored,
            };
            if MAKES.contains(&name.as_str()) && api.objects[index].constructor.is_none() {
                api.objects[index].constructor = Some(Function {
                    name: String::from("new"),
                    returns: Ty::Object(object.clone()),
                    ..made
                });
            } else {
                api.objects[index].statics.push(made);
            }
            continue;
        }
        api.functions.push(Function {
            name: short(library),
            c: function.name.clone(),
            docs: function.docs.clone(),
            params,
            returns,
            status,
            free,
            borrowed: false,
            ignored,
        });
    }
    api.callbacks = context.callbacks.into_inner();
    api.review.extend(context.lent.into_inner());
    let kept = context.kept.into_inner();
    for callback in &api.callbacks {
        if let Some(stop) = callback.stop {
            let when = if kept.contains(&callback.name) {
                "on every call"
            } else {
                "when the Haxe function throws"
            };
            api.review.push(format!(
                "{}: {when}, C is returned {stop} ([HxnStop={stop}])",
                callback.name
            ));
        }
    }
    api
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_lose_the_prefix_they_share() {
        let names =
            |list: &[&str]| value_names(&list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(names(&["MODE_COUNT", "MODE_SUM"]), ["Count", "Sum"]);
        assert_eq!(names(&["MODE_MAX", "MODE_MIN"]), ["Max", "Min"]);
        assert_eq!(names(&["KEY_1", "KEY_2"]), ["Key1", "Key2"]);
        assert_eq!(names(&["ONLY"]), ["Only"]);
    }

    #[test]
    fn a_name_is_made_snake_case() {
        assert_eq!(snake("zDbName"), "z_db_name");
        assert_eq!(snake("HTTPServer"), "http_server");
        assert_eq!(snake("N"), "n");
        assert_eq!(snake("deflateInit2"), "deflate_init2");
        assert_eq!(snake("already_snake"), "already_snake");
    }

    #[test]
    fn a_type_loses_the_library_prefix() {
        assert_eq!(type_name("tally", "tally"), "Tally");
        assert_eq!(type_name("tally", "tally_mode_t"), "Mode");
        assert_eq!(type_name("tally", "point"), "Point");
    }
}
