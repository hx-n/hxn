//! What a library exposes, as every emitter reads it.

use std::fmt;

/// A library's marked API.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Library {
    /// The name its binaries and symbols carry: `counter` for
    /// `counter.hdll` and `libcounter.a`.
    pub name: String,
    pub interfaces: Vec<Interface>,
    /// The `#[hxn::data]` structs: values that cross whole, by copy.
    pub records: Vec<Record>,
    /// The `#[hxn::data]` enums whose variants hold nothing: names for
    /// numbers.
    pub enums: Vec<Enumeration>,
    /// The `#[hxn::data]` enums with a variant that holds values.
    pub tagged: Vec<Tagged>,
}

/// A struct of public fields, each plain data: a Haxe class of the same.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Record {
    pub name: String,
    pub docs: Vec<String>,
    pub fields: Vec<Field>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    pub name: String,
    pub docs: Vec<String>,
    pub ty: Data,
}

/// An enum whose variants hold nothing: a Haxe `enum abstract` over `Int`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Enumeration {
    pub name: String,
    pub docs: Vec<String>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Variant {
    pub name: String,
    pub docs: Vec<String>,
    /// What it is as a number, on both sides.
    pub value: i32,
}

/// An enum with a variant that holds values: a Haxe `enum` of the same
/// cases.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tagged {
    pub name: String,
    pub docs: Vec<String>,
    pub cases: Vec<Case>,
}

/// One variant of a [`Tagged`]. On the wire it is its place among them,
/// counted from 0.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Case {
    pub name: String,
    pub docs: Vec<String>,
    /// What it holds, in order. Fields Rust leaves unnamed are `value`, or
    /// `value0`, `value1` when there is more than one.
    pub fields: Vec<Field>,
    /// Written `Name { field: T }` in Rust, and not `Name(T)`.
    pub named: bool,
}

/// Plain data: what a record's field holds, and what crosses by value. An
/// enum of names crosses as its number. Anything made of parts crosses as
/// bytes that hold the parts in order, which `hxn_abi::wire` lays out.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Data {
    Bool,
    I32,
    I64,
    F64,
    /// A number Rust has and Haxe does not, converted as one is anywhere.
    Number(Number),
    Text,
    Bytes,
    Enum(String),
    Record(String),
    /// An enum whose variants hold values.
    Tagged(String),
    Option(Box<Data>),
    List(Box<Data>),
    /// `Box<T>`, which is its `T` everywhere but in Rust: how data holds
    /// more of its own kind.
    Boxed(Box<Data>),
    /// A `HashMap`, or with `sorted` a `BTreeMap`: a Haxe `Map`. A key is
    /// text or a whole number of at most 32 bits.
    Map {
        key: Box<Data>,
        value: Box<Data>,
        sorted: bool,
    },
}

impl Data {
    /// What it crosses as when it is a parameter or a result of its own:
    /// `None` for what has a way across already.
    pub fn wire(&self) -> Option<Type> {
        match self {
            Self::Enum(_) => Some(Type::I32),
            Self::Record(_)
            | Self::Tagged(_)
            | Self::Option(_)
            | Self::List(_)
            | Self::Map { .. } => Some(Type::Bytes),
            _ => None,
        }
    }

    /// Its name in the manifest.
    pub fn name(&self) -> String {
        match self {
            Self::Bool => String::from("bool"),
            Self::I32 => String::from("i32"),
            Self::I64 => String::from("i64"),
            Self::F64 => String::from("f64"),
            Self::Number(number) => number.name().to_owned(),
            Self::Text => String::from("str"),
            Self::Bytes => String::from("bytes"),
            Self::Enum(name) | Self::Record(name) | Self::Tagged(name) => name.clone(),
            Self::Option(of) => format!("option<{}>", of.name()),
            Self::List(of) => format!("list<{}>", of.name()),
            Self::Boxed(of) => of.name(),
            Self::Map { key, value, .. } => format!("map<{}, {}>", key.name(), value.name()),
        }
    }

    /// What it is called in a symbol, where it is carried by a result over
    /// time: `point`, `list_point`.
    pub fn symbol(&self) -> String {
        match self {
            Self::Bool => String::from("bool"),
            Self::I32 => String::from("int"),
            Self::I64 => String::from("long"),
            Self::F64 => String::from("float"),
            Self::Number(number) => number.name().to_owned(),
            Self::Text => String::from("text"),
            Self::Bytes => String::from("bytes"),
            Self::Enum(name) | Self::Record(name) | Self::Tagged(name) => crate::names::snake(name),
            Self::Option(of) => format!("option_{}", of.symbol()),
            Self::List(of) => format!("list_{}", of.symbol()),
            Self::Boxed(of) => format!("box_{}", of.symbol()),
            Self::Map { key, value, .. } => format!("map_{}_{}", key.symbol(), value.symbol()),
        }
    }

    /// What a value of its own crosses as, a number among them.
    pub fn crosses(&self) -> Option<Type> {
        match self {
            Self::Number(number) => Some(number.wire()),
            other => other.wire(),
        }
    }
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
    /// Not a struct of the library's: a class that is only where its
    /// `#[hxn::function]`s are found. Nothing is made of it or released.
    pub namespace: bool,
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
    /// An `async fn`. The adapter starts it and hands back a future of what
    /// it gives, which is what `returns` is; `fallible` is then whether it
    /// gives a `Result`, whose error fails the future and not the call.
    pub asynchronous: bool,
    /// Marked `#[hxn(blocking)]`: the collector may run while it waits.
    pub blocking: bool,
    /// `new`, which makes the value a Haxe `new` wraps.
    pub constructor: bool,
    /// Returns a shared interface as the `Arc` it is already in, and not
    /// as a value for the adapter to put in one.
    pub returns_arc: bool,
    /// The number the function returns, when `returns` is what it crosses as.
    pub returns_number: Option<Number>,
    /// The data the function returns, when `returns` is what it crosses as.
    pub returns_data: Option<Data>,
    /// The data the function fails with, when its `Result`'s error is the
    /// library's own `#[hxn::data]`: Haxe is thrown an exception that holds
    /// the value.
    pub error: Option<Data>,
    /// A function outside an `impl`, marked `#[hxn::function]`: found at the
    /// crate's root and not on the interface.
    pub free: bool,
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
    /// The number the function takes, when `ty` is what it crosses as.
    pub number: Option<Number>,
    /// The data the function takes, when `ty` is what it crosses as.
    pub data: Option<Data>,
    /// For data, taken by reference: `&Options`, or `&[T]` for a list.
    pub borrowed: bool,
}

/// A number Rust has and the boundary does not. It crosses as an `i32`, an
/// `i64` or an `f64` and the adapter converts it, failing where it does not
/// fit: a parameter, or a result, and not what a callback or a result over
/// time carries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Number {
    I8,
    I16,
    U8,
    U16,
    /// The same 32 bits as the `i32` it crosses as: a Haxe `UInt`.
    U32,
    Isize,
    Usize,
    /// The same 64 bits as the `i64` it crosses as.
    U64,
    F32,
}

impl Number {
    const ALL: [Self; 9] = [
        Self::I8,
        Self::I16,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::Isize,
        Self::Usize,
        Self::U64,
        Self::F32,
    ];

    /// The one Rust calls `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|number| number.name() == name)
    }

    /// What Rust and the manifest call it.
    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::Isize => "isize",
            Self::Usize => "usize",
            Self::U64 => "u64",
            Self::F32 => "f32",
        }
    }

    /// What it crosses as.
    pub fn wire(self) -> Type {
        match self {
            Self::F32 => Type::F64,
            Self::U64 => Type::I64,
            _ => Type::I32,
        }
    }
}

impl Param {
    /// The same parameter, taking `data`.
    pub fn of(mut self, data: Data) -> Self {
        self.data = Some(data);
        self
    }

    /// Its type's name in the manifest.
    pub fn type_name(&self) -> String {
        if let Some(data) = &self.data {
            return data.name();
        }
        self.number
            .map_or_else(|| self.ty.name(), |number| number.name().to_owned())
    }
}

impl Method {
    /// Its result's name in the manifest.
    pub fn returns_name(&self) -> String {
        if let Some(data) = &self.returns_data {
            return data.name();
        }
        self.returns_number
            .map_or_else(|| self.returns.name(), |number| number.name().to_owned())
    }
}

/// The types that cross. A parameter is borrowed for the call and a result
/// is handed over, so `Str` is `&str` coming in and `String` going out,
/// `Bytes` is `&[u8]` and `Vec<u8>`, and
/// an interface is `&T` or `&mut T` coming in and `T` going out. A shared
/// interface may also be `Arc<T>` either way.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    Void,
    Bool,
    I32,
    /// A Haxe `haxe.Int64`: a parameter or a result, and not what a
    /// callback or a result over time carries.
    I64,
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
    /// One result, later: `Future<T>`, a result only.
    Future(Box<Type>),
    /// Many results in order: `Stream<T>`, a result only.
    Stream(Box<Type>),
    /// A value that changes: `Signal<T>`, a result only.
    Signal(Box<Type>),
    /// A Haxe function lent for the call: `impl Fn(..) -> Result<T, E>`, a
    /// parameter only.
    Lent {
        params: Vec<Type>,
        returns: Box<Type>,
    },
    /// Plain data, where a result over time carries it or a lent function
    /// takes or gives it. A parameter or a result of data is what it
    /// crosses as, with the data noted beside it.
    Data(Data),
}

impl Library {
    /// Whether any function returns a future, a stream or a signal, which
    /// takes `hxn-future` in the adapter.
    pub fn returns_later(&self) -> bool {
        self.interfaces
            .iter()
            .flat_map(|interface| &interface.methods)
            .any(|method| {
                matches!(
                    method.returns,
                    Type::Future(_) | Type::Stream(_) | Type::Signal(_)
                )
            })
    }

    /// The data the library's functions fail with, in the order met. Each
    /// is thrown to Haxe as an exception of its own class, and is told from
    /// the others on the way by its place here.
    pub fn errors(&self) -> Vec<&Data> {
        let mut out: Vec<&Data> = Vec::new();
        for method in self.interfaces.iter().flat_map(|i| &i.methods) {
            if let Some(error) = &method.error
                && !out.contains(&error)
            {
                out.push(error);
            }
        }
        out
    }

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
            Self::I64 => String::from("i64"),
            Self::F64 => String::from("f64"),
            Self::Str => String::from("str"),
            Self::Bytes => String::from("bytes"),
            Self::AtomicInt => String::from("atomic_i32"),
            Self::AtomicBool => String::from("atomic_flag"),
            Self::Future(of) => format!("future<{}>", of.name()),
            Self::Stream(of) => format!("stream<{}>", of.name()),
            Self::Signal(of) => format!("signal<{}>", of.name()),
            Self::Lent { params, returns } => {
                let taken: Vec<String> = params.iter().map(Self::name).collect();
                format!("fn({}) -> {}", taken.join(", "), returns.name())
            }
            Self::Data(data) => data.name(),
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
    /// The file, when the library was read from files and not from text.
    pub file: Option<String>,
    /// 1-based, or zero when the whole source is at fault.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line) {
            (Some(file), 0) => write!(f, "{file}: {}", self.message),
            (Some(file), line) => write!(f, "{file}:{line}: {}", self.message),
            (None, 0) => f.write_str(&self.message),
            (None, line) => write!(f, "line {line}: {}", self.message),
        }
    }
}

impl std::error::Error for Error {}
