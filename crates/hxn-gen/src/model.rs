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
    /// `new`, which makes the value a Haxe `new` wraps.
    pub constructor: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Receiver {
    /// No `self`: a static function, or the constructor.
    None,
    /// `&self`.
    Shared,
    /// `&mut self`.
    Exclusive,
    /// `self`: the value is consumed, and the Haxe object closed.
    Owned,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    /// For an interface, borrowed as `&mut` and not `&`.
    pub exclusive: bool,
}

/// The types that cross. A parameter is borrowed for the call and a result
/// is handed over, so `Str` is `&str` coming in and `String` going out, and
/// an interface is `&T` or `&mut T` coming in and `T` going out.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    Void,
    Bool,
    I32,
    F64,
    Str,
    Bytes,
    /// A value of an interface, or with `nullable` possibly none.
    Handle {
        interface: String,
        nullable: bool,
    },
}

impl Type {
    /// Its name in the manifest.
    pub fn name(&self) -> String {
        match self {
            Self::Void => String::from("void"),
            Self::Bool => String::from("bool"),
            Self::I32 => String::from("i32"),
            Self::F64 => String::from("f64"),
            Self::Str => String::from("str"),
            Self::Bytes => String::from("bytes"),
            Self::Handle {
                interface,
                nullable,
            } => {
                if *nullable {
                    format!("{interface}?")
                } else {
                    interface.clone()
                }
            }
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
