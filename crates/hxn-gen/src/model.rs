//! What a library exposes, as every emitter reads it.

use std::fmt;

/// A library's marked API.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Library {
    /// The name its binaries and symbols carry: `counter` for
    /// `counter.hdll` and `libcounter.a`.
    pub name: String,
    pub interfaces: Vec<Interface>,
}

/// A `#[hxn::interface]` struct with the functions of its
/// `#[hxn::methods]` blocks: a Haxe class.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Interface {
    pub name: String,
    pub docs: Vec<String>,
    pub methods: Vec<Method>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Method {
    pub name: String,
    pub docs: Vec<String>,
    pub receiver: Receiver,
    pub params: Vec<Param>,
    pub returns: Type,
    /// Returns a `Result`, whose error is thrown to Haxe.
    pub fallible: bool,
    /// Marked `#[hxn(blocking)]`: the collector may run while it waits.
    pub blocking: bool,
}

impl Method {
    /// `new`, which makes the value a Haxe `new` wraps.
    pub fn is_constructor(&self) -> bool {
        self.returns == Type::Own
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Receiver {
    /// No `self`: a static function, or the constructor.
    None,
    /// `&self`.
    Shared,
    /// `&mut self`.
    Exclusive,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// The types that cross. A parameter is borrowed for the call and a result
/// is handed over, so `Str` is `&str` coming in and `String` going out.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    Void,
    Bool,
    I32,
    F64,
    Str,
    Bytes,
    /// The interface itself, from its constructor.
    Own,
}

impl Type {
    /// Its name in the manifest.
    pub fn name(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Bool => "bool",
            Self::I32 => "i32",
            Self::F64 => "f64",
            Self::Str => "str",
            Self::Bytes => "bytes",
            Self::Own => "self",
        }
    }
}

/// Why a source could not be read, and where.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Error {
    /// 1-based, or zero when the whole source is at fault.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for Error {}
