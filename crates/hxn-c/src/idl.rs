//! The WebIDL form of an [`Api`]: written by the importer, edited by hand,
//! and read back to generate the bindings.
//!
//! | C | WebIDL |
//! |---|---|
//! | an enum | `[HxnEnum] interface Name { const long Value = 1; };` |
//! | a struct passed by value | `dictionary Name { required double x; };` |
//! | an opaque type and its functions | `[HxnFree=c_free] interface Name { ... };` |
//! | the C function an operation calls | `[HxnC=c_function]` on the operation |
//! | a free function that returns an `int`, which is ignored | `[HxnFreeInt]` beside `[HxnFree]` on the interface |
//! | an object the library allows calls on from several threads at once | `[HxnThreadSafe]` on the interface |
//! | a returned object the caller must not free | `[HxnBorrowed]` on the operation |
//! | a function that returns `0` on success | `[HxnStatus]`; `[HxnStatus=negative]` when only a negative value is a failure |
//! | returned text the caller frees | `[HxnFree=c_free]` on the operation |
//! | `long`, `unsigned long`, `size_t`, `ssize_t` | `[HxnC=long]` and so on on a 64-bit type; `[HxnReturns=...]` on an operation for its return type |
//! | a `const` pointer to a struct | `[HxnRef]` on the argument |
//! | a pointer C writes a result through | `[HxnOut]` on the argument, whose type is the operation's return type |
//! | an output nobody reads | `[HxnNull]` on the argument: C is passed a null pointer |
//! | `const char *` | `DOMString`, and `DOMString?` as a return type |
//! | `const uint8_t *` and a `size_t` length | `Uint8Array` |
//! | free functions | `namespace <Library> { ... };` |
//! | a function pointer passed with a `void *` for user data | `callback Name = long (long value);`, and one argument of type `Name` |
//! | where C passes the user data to the function pointer | `[HxnUser=first]` or `[HxnUser=last]` on the callback |
//! | what C is returned when the Haxe function throws | `[HxnStop=1]` on the callback |
//! | a function pointer C keeps, to call after the function returns | `[HxnKept]` on the argument: the operation returns `undefined`, and its binding returns a stream of what C calls it with |
//! | what C is returned by a callback it keeps, on every call | `[HxnStop=0]` on the callback |
//! | the return value of a function that keeps a callback, which is ignored | `[HxnIgnore=int]` or `[HxnIgnore=pointer]` on the operation |
//!
//! A `//` comment directly above a definition or a member is its
//! documentation, which the bindings carry through to Haxe.
//!
//! [`parse`] reads only this subset of WebIDL, and reports the line of
//! anything else.

use crate::model::{
    Api, Callback, Enum, Function, Ignored, Int, Object, Param, Role, Status, Struct, Ty,
};

const HEADER: &str = "// The API of a C library, for hxn. hxn generates the library's Rust bindings\n// from this file: edit it to record what the header cannot, such as which\n// function frees a returned pointer, or [HxnStatus] on a function whose\n// return value is a status code.\n";

fn comment(docs: &[String], indent: &str) -> String {
    docs.iter()
        .map(|line| {
            if line.is_empty() {
                format!("{indent}//\n")
            } else {
                format!("{indent}// {line}\n")
            }
        })
        .collect()
}

fn int_type(int: Int) -> &'static str {
    match int {
        Int::I8 => "byte",
        Int::U8 => "octet",
        Int::I16 => "short",
        Int::U16 => "unsigned short",
        Int::I32 => "long",
        Int::U32 => "unsigned long",
        Int::I64 | Int::Long | Int::SSize => "long long",
        Int::U64 | Int::ULong | Int::Size => "unsigned long long",
    }
}

fn type_name(ty: &Ty) -> String {
    match ty {
        Ty::Void => String::from("undefined"),
        Ty::Bool => String::from("boolean"),
        Ty::Int(int) => int_type(*int).to_owned(),
        Ty::F32 => String::from("float"),
        Ty::F64 => String::from("double"),
        Ty::Text => String::from("DOMString"),
        Ty::Bytes => String::from("Uint8Array"),
        Ty::Enum(name) | Ty::Struct(name) | Ty::StructRef(name) | Ty::Object(name) => name.clone(),
        Ty::Nullable(of) => format!("{}?", type_name(of)),
        Ty::Callback(name) => name.clone(),
    }
}

// The extended attributes a value of `ty` needs where it is declared.
fn value_notes(ty: &Ty) -> Vec<String> {
    match ty {
        Ty::Int(int) => int
            .alias()
            .map(|c| format!("HxnC={c}"))
            .into_iter()
            .collect(),
        Ty::StructRef(_) => vec![String::from("HxnRef")],
        _ => Vec::new(),
    }
}

fn notes(list: &[String]) -> String {
    if list.is_empty() {
        String::new()
    } else {
        format!("[{}] ", list.join(", "))
    }
}

fn arguments(params: &[Param]) -> String {
    let list: Vec<String> = params
        .iter()
        .map(|param| {
            let mut list = value_notes(&param.ty);
            match param.role {
                Role::In => {}
                Role::Out => list.insert(0, String::from("HxnOut")),
                Role::Null => list.insert(0, String::from("HxnNull")),
                Role::Kept => list.insert(0, String::from("HxnKept")),
            }
            format!("{}{} {}", notes(&list), type_name(&param.ty), param.name)
        })
        .collect();
    list.join(", ")
}

fn operation(function: &Function, modifier: &str) -> String {
    let mut list = vec![format!("HxnC={}", function.c)];
    match function.status {
        Status::None => {}
        Status::Zero => list.push(String::from("HxnStatus")),
        Status::Negative => list.push(String::from("HxnStatus=negative")),
    }
    if let Some(free) = &function.free {
        list.push(format!("HxnFree={free}"));
    }
    if function.borrowed {
        list.push(String::from("HxnBorrowed"));
    }
    match function.ignored {
        Ignored::None => {}
        Ignored::Int => list.push(String::from("HxnIgnore=int")),
        Ignored::Pointer => list.push(String::from("HxnIgnore=pointer")),
    }
    if let Ty::Int(int) = &function.returns
        && let Some(c) = int.alias()
    {
        list.push(format!("HxnReturns={c}"));
    }
    format!(
        "{}  {}{modifier}{} {}({});\n",
        comment(&function.docs, "  "),
        notes(&list),
        type_name(&function.returns),
        function.name,
        arguments(&function.params)
    )
}

/// The WebIDL form of `api`.
pub fn write(api: &Api) -> String {
    let mut parts = Vec::new();
    for callback in &api.callbacks {
        let mut out = comment(&callback.docs, "");
        let mut list = vec![String::from(if callback.user_first {
            "HxnUser=first"
        } else {
            "HxnUser=last"
        })];
        if let Some(stop) = callback.stop {
            list.push(format!("HxnStop={stop}"));
        }
        out.push_str(&format!(
            "[{}]\ncallback {} = {} ({});\n",
            list.join(", "),
            callback.name,
            type_name(&callback.returns),
            arguments(&callback.params)
        ));
        parts.push(out);
    }
    for known in &api.enums {
        let mut out = comment(&known.docs, "");
        out.push_str(&format!("[HxnEnum]\ninterface {} {{\n", known.name));
        for (name, value) in &known.values {
            out.push_str(&format!("  const long {name} = {value};\n"));
        }
        out.push_str("};\n");
        parts.push(out);
    }
    for record in &api.structs {
        let mut out = comment(&record.docs, "");
        out.push_str(&format!("dictionary {} {{\n", record.name));
        for field in &record.fields {
            out.push_str(&format!(
                "  {}required {} {};\n",
                notes(&value_notes(&field.ty)),
                type_name(&field.ty),
                field.name
            ));
        }
        out.push_str("};\n");
        parts.push(out);
    }
    for object in &api.objects {
        let mut out = comment(&object.docs, "");
        let mut list = Vec::new();
        if let Some(free) = &object.free {
            list.push(format!("HxnFree={free}"));
            if object.free_int {
                list.push(String::from("HxnFreeInt"));
            }
        }
        if object.thread_safe {
            list.push(String::from("HxnThreadSafe"));
        }
        if !list.is_empty() {
            out.push_str(&format!("[{}]\n", list.join(", ")));
        }
        out.push_str(&format!("interface {} {{\n", object.name));
        if let Some(constructor) = &object.constructor {
            out.push_str(&comment(&constructor.docs, "  "));
            let status = if constructor.status == Status::Zero {
                ", HxnStatus"
            } else {
                ""
            };
            out.push_str(&format!(
                "  [HxnC={}{status}] constructor({});\n",
                constructor.c,
                arguments(&constructor.params)
            ));
        }
        for method in &object.methods {
            out.push_str(&operation(method, ""));
        }
        for function in &object.statics {
            out.push_str(&operation(function, "static "));
        }
        out.push_str("};\n");
        parts.push(out);
    }
    if !api.functions.is_empty() {
        let mut out = format!("namespace {} {{\n", namespace(&api.library));
        for function in &api.functions {
            out.push_str(&operation(function, ""));
        }
        out.push_str("};\n");
        parts.push(out);
    }
    let mut out = String::from(HEADER);
    for part in parts {
        out.push('\n');
        out.push_str(&part);
    }
    if !api.review.is_empty() {
        out.push_str("\n// Check:\n");
        for note in &api.review {
            out.push_str(&format!("// - {note}\n"));
        }
    }
    if !api.skipped.is_empty() {
        out.push_str("\n// Not imported:\n");
        for reason in &api.skipped {
            out.push_str(&format!("// - {reason}\n"));
        }
    }
    out
}

/// The namespace a library's free functions are in: its name in Pascal case.
pub fn namespace(library: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for letter in library.chars() {
        if letter == '_' || letter == '-' {
            upper = true;
        } else if upper {
            out.extend(letter.to_uppercase());
            upper = false;
        } else {
            out.push(letter);
        }
    }
    out
}

#[derive(Clone, PartialEq, Debug)]
enum Token {
    Word(String),
    Number(i64),
    Symbol(char),
}

struct Lexed {
    token: Token,
    line: usize,
    /// The `//` comments on the lines directly above it.
    docs: Vec<String>,
}

fn lex(text: &str) -> Result<Vec<Lexed>, String> {
    let mut out = Vec::new();
    let mut docs: Vec<String> = Vec::new();
    // The line the last comment was on, to tell a comment directly above a
    // token from one separated by a blank line.
    let mut last_comment = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let mut line = 1;
    while at < chars.len() {
        let letter = chars[at];
        if letter == '\n' {
            line += 1;
            at += 1;
            if last_comment + 1 < line {
                docs.clear();
            }
            continue;
        }
        if letter.is_whitespace() {
            at += 1;
            continue;
        }
        if letter == '/' && chars.get(at + 1) == Some(&'/') {
            let end = chars[at..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |n| at + n);
            let body: String = chars[at + 2..end].iter().collect();
            if last_comment + 1 < line {
                docs.clear();
            }
            docs.push(
                body.strip_prefix(' ')
                    .unwrap_or(&body)
                    .trim_end()
                    .to_owned(),
            );
            last_comment = line;
            at = end;
            continue;
        }
        if letter == '/' && chars.get(at + 1) == Some(&'*') {
            let mut end = at + 2;
            while end + 1 < chars.len() && !(chars[end] == '*' && chars[end + 1] == '/') {
                if chars[end] == '\n' {
                    line += 1;
                }
                end += 1;
            }
            at = end + 2;
            docs.clear();
            continue;
        }
        let token = if letter.is_ascii_alphabetic() || letter == '_' {
            let end = chars[at..]
                .iter()
                .position(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
                .map_or(chars.len(), |n| at + n);
            let word: String = chars[at..end].iter().collect();
            at = end;
            Token::Word(word)
        } else if letter.is_ascii_digit()
            || (letter == '-' && chars.get(at + 1).is_some_and(char::is_ascii_digit))
        {
            let end = chars[at + 1..]
                .iter()
                .position(|c| !(c.is_ascii_alphanumeric()))
                .map_or(chars.len(), |n| at + 1 + n);
            let digits: String = chars[at..end].iter().collect();
            at = end;
            let (negative, digits) = match digits.strip_prefix('-') {
                Some(rest) => (true, rest.to_owned()),
                None => (false, digits),
            };
            let value = match digits
                .strip_prefix("0x")
                .or_else(|| digits.strip_prefix("0X"))
            {
                Some(hex) => i64::from_str_radix(hex, 16),
                None => digits.parse::<i64>(),
            }
            .map_err(|_| format!("line {line}: `{digits}` is not a number"))?;
            Token::Number(if negative { -value } else { value })
        } else if "{}()[];,=?".contains(letter) {
            at += 1;
            Token::Symbol(letter)
        } else {
            return Err(format!("line {line}: unexpected `{letter}`"));
        };
        out.push(Lexed {
            token,
            line,
            docs: std::mem::take(&mut docs),
        });
    }
    Ok(out)
}

/// An extended attribute: `[Name]` or `[Name=value]`.
struct Note {
    name: String,
    value: Option<String>,
    line: usize,
}

struct Parser {
    tokens: Vec<Lexed>,
    at: usize,
    /// Each callback argument read: the callback, its line, and whether C
    /// keeps it. Checked once every callback is read.
    uses: Vec<(String, usize, Role)>,
}

impl Parser {
    fn line(&self) -> usize {
        self.tokens
            .get(self.at)
            .or_else(|| self.tokens.last())
            .map_or(1, |lexed| lexed.line)
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, String> {
        Err(format!("line {}: {}", self.line(), message.into()))
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|lexed| &lexed.token)
    }

    fn docs(&self) -> Vec<String> {
        self.tokens
            .get(self.at)
            .map(|lexed| lexed.docs.clone())
            .unwrap_or_default()
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.peek().cloned();
        self.at += 1;
        token
    }

    fn is(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w == word)
    }

    fn is_symbol(&self, symbol: char) -> bool {
        self.peek() == Some(&Token::Symbol(symbol))
    }

    fn symbol(&mut self, symbol: char) -> Result<(), String> {
        if self.is_symbol(symbol) {
            self.at += 1;
            Ok(())
        } else {
            self.fail(format!("expected `{symbol}`"))
        }
    }

    fn word(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(Token::Word(word)) => {
                let word = word.clone();
                self.at += 1;
                Ok(word)
            }
            _ => self.fail("expected a name"),
        }
    }

    fn keyword(&mut self, word: &str) -> Result<(), String> {
        if self.is(word) {
            self.at += 1;
            Ok(())
        } else {
            self.fail(format!("expected `{word}`"))
        }
    }

    fn notes(&mut self) -> Result<Vec<Note>, String> {
        let mut out = Vec::new();
        if !self.is_symbol('[') {
            return Ok(out);
        }
        self.at += 1;
        loop {
            let line = self.line();
            let name = self.word()?;
            let value = if self.is_symbol('=') {
                self.at += 1;
                match self.next() {
                    Some(Token::Word(word)) => Some(word),
                    Some(Token::Number(number)) => Some(number.to_string()),
                    _ => return self.fail(format!("`{name}=` needs a value")),
                }
            } else {
                None
            };
            out.push(Note { name, value, line });
            if self.is_symbol(',') {
                self.at += 1;
                continue;
            }
            self.symbol(']')?;
            return Ok(out);
        }
    }

    // A type, as the names it is written with: `unsigned long long`, `Name?`.
    fn type_words(&mut self) -> Result<(String, bool), String> {
        let mut words = vec![self.word()?];
        if words[0] == "unsigned" {
            words.push(self.word()?);
        }
        if words.last().is_some_and(|w| w == "long") && self.is("long") {
            self.at += 1;
            words.push(String::from("long"));
        }
        let nullable = if self.is_symbol('?') {
            self.at += 1;
            true
        } else {
            false
        };
        Ok((words.join(" "), nullable))
    }
}

/// Where each name is declared, to resolve a type written by name.
#[derive(Default)]
struct Names {
    enums: Vec<String>,
    structs: Vec<String>,
    objects: Vec<String>,
    callbacks: Vec<String>,
}

fn find<'a>(notes: &'a [Note], name: &str) -> Option<&'a Note> {
    notes.iter().find(|note| note.name == name)
}

fn check_notes(notes: &[Note], allowed: &[&str], what: &str) -> Result<(), String> {
    for note in notes {
        if !allowed.contains(&note.name.as_str()) {
            return Err(format!(
                "line {}: `[{}]` does not apply to {what}",
                note.line, note.name
            ));
        }
    }
    Ok(())
}

// The type `words` names, with the `[HxnC=...]` and `[HxnRef]` notes on it.
fn resolve(
    words: &str,
    nullable: bool,
    notes: &[Note],
    names: &Names,
    line: usize,
) -> Result<Ty, String> {
    let fail = |message: String| Err(format!("line {line}: {message}"));
    let alias = find(notes, "HxnC").map(|note| note.value.clone().unwrap_or_default());
    let ty = match words {
        "undefined" => Ty::Void,
        "boolean" => Ty::Bool,
        "byte" => Ty::Int(Int::I8),
        "octet" => Ty::Int(Int::U8),
        "short" => Ty::Int(Int::I16),
        "unsigned short" => Ty::Int(Int::U16),
        "long" => Ty::Int(Int::I32),
        "unsigned long" => Ty::Int(Int::U32),
        "long long" => match alias.as_deref() {
            None => Ty::Int(Int::I64),
            Some("long") => Ty::Int(Int::Long),
            Some("ssize_t") => Ty::Int(Int::SSize),
            Some(other) => {
                return fail(format!(
                    "`[HxnC={other}]` is not a signed 64-bit C type: use `long` or `ssize_t`"
                ));
            }
        },
        "unsigned long long" => match alias.as_deref() {
            None => Ty::Int(Int::U64),
            Some("unsigned_long") => Ty::Int(Int::ULong),
            Some("size_t") => Ty::Int(Int::Size),
            Some(other) => {
                return fail(format!(
                    "`[HxnC={other}]` is not an unsigned 64-bit C type: use `unsigned_long` or `size_t`"
                ));
            }
        },
        "float" | "unrestricted float" => Ty::F32,
        "double" | "unrestricted double" => Ty::F64,
        "DOMString" | "USVString" | "ByteString" => Ty::Text,
        "Uint8Array" => Ty::Bytes,
        name if names.enums.iter().any(|known| known == name) => Ty::Enum(name.to_owned()),
        name if names.structs.iter().any(|known| known == name) => {
            if find(notes, "HxnRef").is_some() {
                Ty::StructRef(name.to_owned())
            } else {
                Ty::Struct(name.to_owned())
            }
        }
        name if names.objects.iter().any(|known| known == name) => Ty::Object(name.to_owned()),
        name if names.callbacks.iter().any(|known| known == name) => Ty::Callback(name.to_owned()),
        other => {
            return fail(format!(
                "`{other}` is not a type this file declares or one hxn maps"
            ));
        }
    };
    if alias.is_some() && !matches!(ty, Ty::Int(Int::Long | Int::ULong | Int::Size | Int::SSize)) {
        return fail(String::from(
            "`[HxnC=...]` on a type applies only to `long long` and `unsigned long long`",
        ));
    }
    if nullable {
        return match ty {
            Ty::Text | Ty::Object(_) => Ok(Ty::Nullable(Box::new(ty))),
            _ => fail(format!("`{words}?`: only text and objects may be null")),
        };
    }
    Ok(ty)
}

fn arguments_of(parser: &mut Parser, names: &Names) -> Result<Vec<Param>, String> {
    parser.symbol('(')?;
    let mut params = Vec::new();
    while !parser.is_symbol(')') {
        let notes = parser.notes()?;
        check_notes(
            &notes,
            &["HxnC", "HxnRef", "HxnOut", "HxnNull", "HxnKept"],
            "an argument",
        )?;
        let line = parser.line();
        let (words, nullable) = parser.type_words()?;
        let ty = resolve(&words, nullable, &notes, names, line)?;
        if matches!(ty, Ty::Void) {
            return parser.fail("an argument cannot be `undefined`");
        }
        let roles: Vec<Role> = [
            ("HxnOut", Role::Out),
            ("HxnNull", Role::Null),
            ("HxnKept", Role::Kept),
        ]
        .into_iter()
        .filter(|(note, _)| find(&notes, note).is_some())
        .map(|(_, role)| role)
        .collect();
        let role = match roles.as_slice() {
            [] => Role::In,
            [role] => *role,
            _ => {
                return parser.fail(
                    "an argument is one of `[HxnOut]`, `[HxnNull]` and `[HxnKept]`, not several",
                );
            }
        };
        match (role, &ty) {
            (Role::Kept, Ty::Callback(_)) | (Role::In, _) => {}
            (Role::Kept, _) => {
                return parser.fail("`[HxnKept]` applies to a callback argument");
            }
            (_, Ty::Bytes | Ty::StructRef(_) | Ty::Callback(_)) => {
                return parser.fail(format!(
                    "`{words}` cannot be written through a pointer by C"
                ));
            }
            _ => {}
        }
        if let Ty::Callback(known) = &ty {
            parser.uses.push((known.clone(), line, role));
        }
        let name = parser.word()?;
        params.push(Param { name, ty, role });
        if parser.is_symbol(',') {
            parser.at += 1;
        }
    }
    parser.symbol(')')?;
    Ok(params)
}

fn c_name(notes: &[Note], line: usize, what: &str) -> Result<String, String> {
    match find(notes, "HxnC").and_then(|note| note.value.clone()) {
        Some(name) => Ok(name),
        None => Err(format!(
            "line {line}: {what} needs `[HxnC=<C function>]`, the function it calls"
        )),
    }
}

// An operation, after its notes and its docs have been read.
fn operation_of(
    parser: &mut Parser,
    notes: &[Note],
    docs: Vec<String>,
    names: &Names,
) -> Result<Function, String> {
    check_notes(
        notes,
        &[
            "HxnC",
            "HxnStatus",
            "HxnFree",
            "HxnReturns",
            "HxnBorrowed",
            "HxnIgnore",
        ],
        "an operation",
    )?;
    let start = parser.line();
    let line = parser.line();
    let (words, nullable) = parser.type_words()?;
    let returned: Vec<Note> = find(notes, "HxnReturns")
        .map(|note| Note {
            name: String::from("HxnC"),
            value: note.value.clone(),
            line: note.line,
        })
        .into_iter()
        .collect();
    let returns = resolve(&words, nullable, &returned, names, line)?;
    let name = parser.word()?;
    let params = arguments_of(parser, names)?;
    parser.symbol(';')?;
    let status = match find(notes, "HxnStatus") {
        None => Status::None,
        Some(note) => match note.value.as_deref() {
            None => Status::Zero,
            Some("negative") => Status::Negative,
            Some(other) => {
                return Err(format!(
                    "line {}: `[HxnStatus={other}]`: write `[HxnStatus]` for 0 meaning success, or `[HxnStatus=negative]`",
                    note.line
                ));
            }
        },
    };
    check_returns(&name, &params, &returns, status, start)?;
    if find(notes, "HxnBorrowed").is_some() && returns.inner_object().is_none() {
        return Err(format!(
            "line {line}: `[HxnBorrowed]` applies to an operation that returns an object, and `{name}` returns `{words}`"
        ));
    }
    let free = find(notes, "HxnFree").and_then(|note| note.value.clone());
    if free.is_some() && returns != Ty::Nullable(Box::new(Ty::Text)) && returns != Ty::Text {
        return Err(format!(
            "line {line}: `[HxnFree]` on an operation frees the text it returns, and `{name}` returns `{words}`"
        ));
    }
    let ignored = match find(notes, "HxnIgnore") {
        None => Ignored::None,
        Some(note) => match note.value.as_deref() {
            Some("int") => Ignored::Int,
            Some("pointer") => Ignored::Pointer,
            _ => {
                return Err(format!(
                    "line {}: the C return value ignored is `[HxnIgnore=int]` or `[HxnIgnore=pointer]`",
                    note.line
                ));
            }
        },
    };
    check_kept(&name, &params, &returns, status, ignored, start)?;
    Ok(Function {
        name,
        c: c_name(notes, line, "an operation")?,
        docs,
        params,
        returns,
        status,
        free,
        borrowed: find(notes, "HxnBorrowed").is_some(),
        ignored,
    })
}

// Whether an operation that keeps a callback, or ignores its C return
// value, has the form the binding generates: the stream it returns is its
// only result.
fn check_kept(
    name: &str,
    params: &[Param],
    returns: &Ty,
    status: Status,
    ignored: Ignored,
    line: usize,
) -> Result<(), String> {
    let fail = |message: String| Err(format!("line {line}: {message}"));
    let kept = params.iter().filter(|p| p.role == Role::Kept).count();
    if kept == 0 {
        if ignored != Ignored::None {
            return fail(format!(
                "`[HxnIgnore]` applies to an operation with an `[HxnKept]` argument, and `{name}` has none"
            ));
        }
        return Ok(());
    }
    if kept > 1 {
        return fail(format!("`{name}` has more than one `[HxnKept]` argument"));
    }
    if *returns != Ty::Void {
        return fail(format!(
            "`{name}` keeps a callback, so it returns `undefined`: its binding returns the stream of what C calls the callback with"
        ));
    }
    if status == Status::Negative || (status == Status::Zero && ignored != Ignored::None) {
        return fail(format!(
            "`{name}` keeps a callback: its C return value is a status, `[HxnStatus]`, or ignored, `[HxnIgnore]`"
        ));
    }
    if let Some(other) = params
        .iter()
        .find(|p| p.role == Role::Out || (p.role == Role::In && matches!(p.ty, Ty::Callback(_))))
    {
        return fail(format!(
            "`{name}` keeps a callback, and so takes no `[HxnOut]` argument and lends no other callback, as `{}` is",
            other.name
        ));
    }
    Ok(())
}

// A callback is lent for a call only if a Haxe function can take what it
// takes; C may call a kept one with any of the values a callback declares.
fn check_uses(api: &Api, uses: &[(String, usize, Role)]) -> Result<(), String> {
    for (name, line, role) in uses {
        let Some(callback) = api.callbacks.iter().find(|known| &known.name == name) else {
            continue;
        };
        if *role == Role::Kept {
            if callback.returns != Ty::Void && callback.stop.is_none() {
                return Err(format!(
                    "line {line}: C keeps `{name}`, which returns a value: give `{name}` `[HxnStop=n]`, what C is returned on every call"
                ));
            }
            continue;
        }
        if callback.params.len() > 4 {
            return Err(format!(
                "line {line}: `{name}` takes {} values, and a Haxe function lent to C takes at most 4. If C keeps it, mark the argument `[HxnKept]`",
                callback.params.len()
            ));
        }
        if let Some(param) = callback.params.iter().find(|param| {
            !matches!(
                param.ty,
                Ty::Bool
                    | Ty::Int(Int::I32)
                    | Ty::F32
                    | Ty::F64
                    | Ty::Text
                    | Ty::Enum(_)
                    | Ty::Struct(_)
            )
        }) {
            return Err(format!(
                "line {line}: a Haxe function lent to C takes `boolean`, `long`, `float`, `double`, `DOMString`, an enum or a dictionary, and `{}` of `{name}` is not one. If C keeps it, mark the argument `[HxnKept]`",
                param.name
            ));
        }
    }
    Ok(())
}

// Whether what an operation returns fits how C gives it: as the return
// value, or written to its `[HxnOut]` argument, with or without a status.
fn check_returns(
    name: &str,
    params: &[Param],
    returns: &Ty,
    status: Status,
    line: usize,
) -> Result<(), String> {
    let fail = |message: String| Err(format!("line {line}: {message}"));
    let outs: Vec<&Param> = params.iter().filter(|p| p.role == Role::Out).collect();
    match outs.as_slice() {
        [] => match (status, returns) {
            (Status::Zero, Ty::Void) | (Status::None, _) | (Status::Negative, Ty::Int(_)) => Ok(()),
            (Status::Zero, Ty::Object(_)) if name == "constructor" => fail(String::from(
                "a constructor with `[HxnStatus]` gets its object from an `[HxnOut]` argument",
            )),
            (Status::Zero, _) => fail(format!(
                "`{name}` has `[HxnStatus]`, so it returns `undefined`: the C function's int is only its status. A value C writes through a pointer is an `[HxnOut]` argument"
            )),
            (Status::Negative, _) => fail(format!(
                "`{name}` has `[HxnStatus=negative]`, so it returns an integer"
            )),
        },
        [out] => {
            if status == Status::Negative {
                return fail(format!(
                    "`{name}` has an `[HxnOut]` argument, so its C return value is a status, `[HxnStatus]`, or nothing"
                ));
            }
            let matches = *returns == out.ty
                || matches!((returns, &out.ty), (Ty::Nullable(of), ty) if **of == *ty)
                || matches!((returns, &out.ty), (ty, Ty::Nullable(of)) if **of == *ty);
            if matches {
                Ok(())
            } else {
                fail(format!(
                    "`{name}` returns its `[HxnOut]` argument `{}`, so its return type is that argument's",
                    out.name
                ))
            }
        }
        _ => fail(format!(
            "`{name}` has more than one `[HxnOut]` argument: return one, and pass `[HxnNull]` for the others"
        )),
    }
}

/// Reads the WebIDL form of a C library's API. `library` names the library,
/// whose namespace holds its free functions.
pub fn parse(library: &str, text: &str) -> Result<Api, String> {
    let tokens = lex(text)?;
    // Every name first, so a definition can use one declared after it.
    let mut names = Names::default();
    {
        let mut enum_next = false;
        let mut depth = 0;
        let mut kind: Option<String> = None;
        for lexed in &tokens {
            match &lexed.token {
                Token::Symbol('{') => depth += 1,
                Token::Symbol('}') => depth -= 1,
                Token::Word(word) if depth == 0 && word == "HxnEnum" => enum_next = true,
                Token::Word(word)
                    if depth == 0
                        && ["interface", "dictionary", "namespace", "callback"]
                            .contains(&word.as_str()) =>
                {
                    kind = Some(word.clone());
                }
                Token::Word(word) if depth == 0 && kind.is_some() => {
                    match kind.take().as_deref() {
                        Some("interface") if enum_next => names.enums.push(word.clone()),
                        Some("interface") => names.objects.push(word.clone()),
                        Some("dictionary") => names.structs.push(word.clone()),
                        Some("callback") => names.callbacks.push(word.clone()),
                        _ => {}
                    }
                    enum_next = false;
                }
                _ => {}
            }
        }
    }

    let mut parser = Parser {
        tokens,
        at: 0,
        uses: Vec::new(),
    };
    let mut api = Api {
        library: library.to_owned(),
        ..Api::default()
    };
    while parser.peek().is_some() {
        let docs = parser.docs();
        let notes = parser.notes()?;
        let line = parser.line();
        let kind = parser.word()?;
        match kind.as_str() {
            "callback" => {
                check_notes(&notes, &["HxnUser", "HxnStop"], "a callback")?;
                let name = parser.word()?;
                parser.symbol('=')?;
                let line = parser.line();
                let (words, nullable) = parser.type_words()?;
                let returns = resolve(&words, nullable, &[], &names, line)?;
                if !matches!(returns, Ty::Void | Ty::Bool | Ty::Int(Int::I32)) {
                    return Err(format!(
                        "line {line}: a callback returns `undefined`, `boolean` or `long`"
                    ));
                }
                let params = arguments_of(&mut parser, &names)?;
                parser.symbol(';')?;
                for param in &params {
                    let fits = matches!(
                        param.ty,
                        Ty::Bool
                            | Ty::Int(_)
                            | Ty::F32
                            | Ty::F64
                            | Ty::Text
                            | Ty::Enum(_)
                            | Ty::Struct(_)
                    );
                    if !fits || param.role != Role::In {
                        return Err(format!(
                            "line {line}: a callback takes a `boolean`, an integer, a `float`, a `double`, a `DOMString`, an enum or a dictionary, and `{}` is not one",
                            param.name
                        ));
                    }
                }
                let user_first = match find(&notes, "HxnUser")
                    .and_then(|note| note.value.as_deref())
                {
                    None | Some("first") => true,
                    Some("last") => false,
                    Some(other) => {
                        return Err(format!(
                            "line {line}: `[HxnUser={other}]`: the user data is `first` or `last`"
                        ));
                    }
                };
                let stop = match find(&notes, "HxnStop") {
                    None => None,
                    Some(note) => {
                        if returns == Ty::Void {
                            return Err(format!(
                                "line {}: `[HxnStop]` is what C is returned, and `{name}` returns nothing",
                                note.line
                            ));
                        }
                        Some(
                            note.value
                                .as_deref()
                                .and_then(|v| v.parse::<i64>().ok())
                                .ok_or_else(|| {
                                    format!("line {}: `[HxnStop=n]` needs a number", note.line)
                                })?,
                        )
                    }
                };
                api.callbacks.push(Callback {
                    name,
                    docs,
                    params,
                    returns,
                    user_first,
                    stop,
                });
            }
            "dictionary" => {
                check_notes(&notes, &[], "a dictionary")?;
                let name = parser.word()?;
                parser.symbol('{')?;
                let mut fields = Vec::new();
                while !parser.is_symbol('}') {
                    let field_notes = parser.notes()?;
                    check_notes(&field_notes, &["HxnC"], "a dictionary member")?;
                    if parser.is("required") {
                        parser.at += 1;
                    }
                    let line = parser.line();
                    let (words, nullable) = parser.type_words()?;
                    let ty = resolve(&words, nullable, &field_notes, &names, line)?;
                    if !matches!(ty, Ty::Bool | Ty::Int(_) | Ty::F32 | Ty::F64) {
                        return Err(format!(
                            "line {line}: a dictionary member is a boolean, an integer or a floating-point number"
                        ));
                    }
                    let field = parser.word()?;
                    parser.symbol(';')?;
                    fields.push(Param::new(field, ty));
                }
                parser.symbol('}')?;
                parser.symbol(';')?;
                api.structs.push(Struct { name, docs, fields });
            }
            "interface" if find(&notes, "HxnEnum").is_some() => {
                check_notes(&notes, &["HxnEnum"], "an enum")?;
                let name = parser.word()?;
                parser.symbol('{')?;
                let mut values = Vec::new();
                while !parser.is_symbol('}') {
                    parser.keyword("const")?;
                    let (words, _) = parser.type_words()?;
                    if words != "long" {
                        return parser.fail("an enum's values are `const long`");
                    }
                    let value_name = parser.word()?;
                    parser.symbol('=')?;
                    let Some(Token::Number(value)) = parser.next() else {
                        return parser.fail("expected the value's number");
                    };
                    parser.symbol(';')?;
                    values.push((value_name, value));
                }
                parser.symbol('}')?;
                parser.symbol(';')?;
                if values.is_empty() {
                    return Err(format!("line {line}: `{name}` has no values"));
                }
                api.enums.push(Enum { name, docs, values });
            }
            "interface" => {
                check_notes(
                    &notes,
                    &["HxnFree", "HxnFreeInt", "HxnThreadSafe"],
                    "an interface",
                )?;
                let name = parser.word()?;
                let mut object = Object {
                    name,
                    docs,
                    free: find(&notes, "HxnFree").and_then(|note| note.value.clone()),
                    free_int: find(&notes, "HxnFreeInt").is_some(),
                    thread_safe: find(&notes, "HxnThreadSafe").is_some(),
                    constructor: None,
                    methods: Vec::new(),
                    statics: Vec::new(),
                };
                parser.symbol('{')?;
                while !parser.is_symbol('}') {
                    let docs = parser.docs();
                    let notes = parser.notes()?;
                    if parser.is("constructor") {
                        let line = parser.line();
                        parser.at += 1;
                        check_notes(&notes, &["HxnC", "HxnStatus"], "a constructor")?;
                        let params = arguments_of(&mut parser, &names)?;
                        parser.symbol(';')?;
                        if params.iter().any(|p| p.role == Role::Kept) {
                            return Err(format!(
                                "line {line}: a constructor returns its object, and so cannot keep a callback"
                            ));
                        }
                        let status = if find(&notes, "HxnStatus").is_some() {
                            Status::Zero
                        } else {
                            Status::None
                        };
                        let own = Ty::Object(object.name.clone());
                        check_returns("constructor", &params, &own, status, line)?;
                        if object.constructor.is_some() {
                            return Err(format!(
                                "line {line}: `{}` has two constructors",
                                object.name
                            ));
                        }
                        object.constructor = Some(Function {
                            name: String::from("new"),
                            c: c_name(&notes, line, "a constructor")?,
                            docs,
                            params,
                            returns: own,
                            status,
                            free: None,
                            borrowed: false,
                            ignored: Ignored::None,
                        });
                    } else if parser.is("static") {
                        parser.at += 1;
                        object
                            .statics
                            .push(operation_of(&mut parser, &notes, docs, &names)?);
                    } else {
                        object
                            .methods
                            .push(operation_of(&mut parser, &notes, docs, &names)?);
                    }
                }
                parser.symbol('}')?;
                parser.symbol(';')?;
                api.objects.push(object);
            }
            "namespace" => {
                check_notes(&notes, &[], "a namespace")?;
                let name = parser.word()?;
                if name != namespace(library) {
                    return Err(format!(
                        "line {line}: the namespace of library `{library}` is `{}`",
                        namespace(library)
                    ));
                }
                parser.symbol('{')?;
                while !parser.is_symbol('}') {
                    let docs = parser.docs();
                    let notes = parser.notes()?;
                    api.functions
                        .push(operation_of(&mut parser, &notes, docs, &names)?);
                }
                parser.symbol('}')?;
                parser.symbol(';')?;
            }
            other => {
                return Err(format!(
                    "line {line}: expected `interface`, `dictionary`, `namespace` or `callback`, found `{other}`"
                ));
            }
        }
    }
    check_uses(&api, &parser.uses)?;
    Ok(api)
}
