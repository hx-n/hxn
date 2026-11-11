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
    /// Marked `#[hxn::interface(shared)]`: a handle holds an `Arc` of the
    /// value, so the library and any number of Haxe objects can hold one.
    pub shared: bool,
    /// Marked `#[hxn::interface(weighed)]`: its values hold memory worth
    /// collecting for, so making one checks what the library holds.
    pub weighed: bool,
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
    /// Returns a shared interface as the `Arc` it is already in, and not
    /// as a value for the adapter to put in one.
    pub returns_arc: bool,
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
    /// For a shared interface, taken as `Arc<T>`: a share the library keeps.
    pub kept: bool,
}

/// The types that cross. A parameter is borrowed for the call and a result
/// is handed over, so `Str` is `&str` coming in and `String` going out, and
/// an interface is `&T` or `&mut T` coming in and `T` going out. A shared
/// interface may also be `Arc<T>` either way.
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
    /// A cell both sides read and write: `&AtomicI32` coming in, lent for
    /// the call, and `Arc<AtomicI32>` going out, kept by the library.
    AtomicInt,
    /// The same for a flag: `&AtomicFlag` and `Arc<AtomicFlag>`.
    AtomicBool,
}

impl Library {
    /// Whether `interface` is one of the shared ones.
    pub fn is_shared(&self, interface: &str) -> bool {
        self.interfaces
            .iter()
            .any(|known| known.shared && known.name == interface)
    }

    /// Whether any function hands the host a cell, which takes a call of
    /// the library's own to reach.
    pub fn returns_cells(&self) -> bool {
        self.interfaces
            .iter()
            .flat_map(|interface| &interface.methods)
            .any(|method| matches!(method.returns, Type::AtomicInt | Type::AtomicBool))
    }
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
            Self::AtomicInt => String::from("atomic_i32"),
            Self::AtomicBool => String::from("atomic_flag"),
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
