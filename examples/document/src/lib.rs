//! Wraps `serde_json` for Haxe: parses and serializes JSON, with values
//! exposed as a recursive enum and parse errors as a typed exception.

use std::collections::BTreeMap;
use std::fmt;

/// A JSON value. Recursive through `Items` and `Fields`; Haxe gets an enum
/// with the same cases.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nothing,
    Flag(bool),
    Number(f64),
    Text(String),
    Items(Vec<Value>),
    Fields(BTreeMap<String, Value>),
}

/// A parse error with its position. Used as the error type of `parse`, so
/// Haxe catches it as `ParseException`.
#[hxn::data]
#[derive(Clone, Debug)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.message, self.line, self.column)
    }
}

fn from_json(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Nothing,
        serde_json::Value::Bool(flag) => Value::Flag(flag),
        serde_json::Value::Number(number) => Value::Number(number.as_f64().unwrap_or(f64::NAN)),
        serde_json::Value::String(text) => Value::Text(text),
        serde_json::Value::Array(items) => Value::Items(items.into_iter().map(from_json).collect()),
        serde_json::Value::Object(fields) => Value::Fields(
            fields
                .into_iter()
                .map(|(name, value)| (name, from_json(value)))
                .collect(),
        ),
    }
}

fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Nothing => serde_json::Value::Null,
        Value::Flag(flag) => serde_json::Value::Bool(*flag),
        // NaN and infinities have no JSON form; serialize them as null.
        Value::Number(number) => serde_json::Number::from_f64(*number)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        Value::Text(text) => serde_json::Value::String(text.clone()),
        Value::Items(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
        Value::Fields(fields) => serde_json::Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), to_json(value)))
                .collect(),
        ),
    }
}

/// Parses `text` as JSON.
#[hxn::function]
pub fn parse(text: &str) -> Result<Value, ParseError> {
    serde_json::from_str(text)
        .map(from_json)
        .map_err(|error| ParseError {
            // serde_json appends " at line L column C"; the position has
            // its own fields.
            message: error
                .to_string()
                .split(" at line ")
                .next()
                .unwrap_or_default()
                .to_owned(),
            line: error.line() as u32,
            column: error.column() as u32,
        })
}

/// Serializes `value` to JSON, compact or pretty-printed.
#[hxn::function]
pub fn write(value: &Value, pretty: bool) -> String {
    let json = to_json(value);
    if pretty {
        format!("{json:#}")
    } else {
        json.to_string()
    }
}

/// Looks up a JSON pointer such as `/tags/1`. Returns `None` if nothing
/// is there.
#[hxn::function]
pub fn at(value: &Value, path: &str) -> Option<Value> {
    to_json(value).pointer(path).cloned().map(from_json)
}
