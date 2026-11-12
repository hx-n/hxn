//! The parsed model of a library's API, shared by all emitters.

use std::fmt;

/// A library's marked API.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Library {
    /// The name used for its binaries and symbol prefixes: `counter` for
    /// `counter.hdll` and `libcounter.a`.
    pub name: String,
    pub interfaces: Vec<Interface>,
    /// The `#[hxn::data]` structs, passed by value (copied).
    pub records: Vec<Record>,
    /// The `#[hxn::data]` C-like enums.
    pub enums: Vec<Enumeration>,
    /// The `#[hxn::data]` tagged enums (at least one variant has fields).
    pub tagged: Vec<Tagged>,
}

/// A struct whose public fields are all plain data. It becomes a Haxe class
/// with the same fields.
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

/// A C-like enum: a Haxe `enum abstract` over `Int`.
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
    /// Its integer value, the same on both sides.
    pub value: i32,
}

/// A tagged enum: a Haxe `enum` with the same cases.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tagged {
    pub name: String,
    pub docs: Vec<String>,
    pub cases: Vec<Case>,
}

/// One variant of a [`Tagged`]. Its wire tag is its 0-based index among the
/// variants.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Case {
    pub name: String,
    pub docs: Vec<String>,
    /// Its fields, in order. Unnamed (tuple) fields are named `value`, or
    /// `value0`, `value1` when there is more than one.
    pub fields: Vec<Field>,
    /// Declared with named fields, `Name { field: T }`, rather than `Name(T)`.
    pub named: bool,
}

/// Plain data: the type of a record field, or of a value passed by value. A
/// C-like enum is passed as its integer value. A composite value is
/// serialized to bytes, its parts in order, in the format `hxn_abi::wire`
/// defines.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Data {
    Bool,
    I32,
    I64,
    F64,
    /// A Rust numeric type with no Haxe equivalent, converted the same way as
    /// everywhere else.
    Number(Number),
    Text,
    Bytes,
    Enum(String),
    Record(String),
    /// A tagged enum.
    Tagged(String),
    Option(Box<Data>),
    List(Box<Data>),
    /// `Box<T>`, treated as `T` everywhere except in Rust: how a recursive
    /// type is declared.
    Boxed(Box<Data>),
    /// A `HashMap`, or a `BTreeMap` when `sorted`: a Haxe `Map`. A key is
    /// text or an integer of at most 32 bits.
    Map {
        key: Box<Data>,
        value: Box<Data>,
        sorted: bool,
    },
}

impl Data {
    /// Its wire type when it is passed directly as a parameter or return
    /// value: `None` for a type the boundary already supports.
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

    /// Its name within a symbol, used for the exports of a future, stream or
    /// signal that carries it: `point`, `list_point`.
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

    /// Like [`Data::wire`], but also gives the wire type of a converted
    /// number.
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
    /// value, so the library and any number of Haxe objects can share it.
    pub shared: bool,
    /// Marked `#[hxn::interface(weighed)]`: its values hold enough memory to
    /// justify a collection, so creating one checks the library's memory
    /// meter.
    pub weighed: bool,
    /// Not a struct of the library: a class that only holds
    /// `#[hxn::function]`s. It is never instantiated or released.
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
    /// An `async fn`. The adapter starts it and returns a future of its
    /// output; `returns` is that future. `fallible` is then whether the
    /// output is a `Result`, whose error rejects the future rather than
    /// failing the call.
    pub asynchronous: bool,
    /// Marked `#[hxn(blocking)]`: the garbage collector may run while it
    /// blocks.
    pub blocking: bool,
    /// `new`, the constructor behind a Haxe `new`.
    pub constructor: bool,
    /// Returns a shared interface already wrapped in an `Arc`, rather than
    /// a value for the adapter to wrap.
    pub returns_arc: bool,
    /// Returns a `Vec` of an interface. It crosses as a stream of its
    /// elements that has already ended, and the Haxe method returns them as
    /// an `Array`; `returns` is that stream.
    pub returns_list: bool,
    /// The numeric type the function returns, when `returns` is its wire type.
    pub returns_number: Option<Number>,
    /// The data type the function returns, when `returns` is its wire type.
    pub returns_data: Option<Data>,
    /// The error type, when the `Result`'s error is one of the library's
    /// `#[hxn::data]` types: Haxe receives a typed exception holding the
    /// value.
    pub error: Option<Data>,
    /// A free function marked `#[hxn::function]`: called by its path at the
    /// crate's root, not as a method of the interface.
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
    /// For a shared interface, accepted as `Arc<T>`: a reference the library
    /// keeps.
    pub kept: bool,
    /// The parameter's numeric type, when `ty` is its wire type.
    pub number: Option<Number>,
    /// The parameter's data type, when `ty` is its wire type.
    pub data: Option<Data>,
    /// For data, accepted by reference: `&Options`, or `&[T]` for a list.
    pub borrowed: bool,
}

/// A Rust numeric type the boundary does not support. It is passed as an
/// `i32`, `i64` or `f64` and the adapter converts it, failing when the value
/// does not fit. Allowed as a parameter, a return value, and in a future,
/// stream or signal; not as a callback's parameter or return value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Number {
    I8,
    I16,
    U8,
    U16,
    /// Reinterprets the 32 bits of its `i32` wire type: a Haxe `UInt`.
    U32,
    Isize,
    Usize,
    /// Reinterprets the 64 bits of its `i64` wire type.
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

    /// The numeric type whose Rust name is `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|number| number.name() == name)
    }

    /// Its name in Rust and in the manifest.
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

    /// Its wire type.
    pub fn wire(self) -> Type {
        match self {
            Self::F32 => Type::F64,
            Self::U64 => Type::I64,
            _ => Type::I32,
        }
    }
}

impl Param {
    /// The same parameter, with `data` as its data type.
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
    /// Its return type's name in the manifest.
    pub fn returns_name(&self) -> String {
        if let Some(data) = &self.returns_data {
            return data.name();
        }
        if let (true, Type::Stream(of)) = (self.returns_list, &self.returns) {
            return format!("list<{}>", of.name());
        }
        self.returns_number
            .map_or_else(|| self.returns.name(), |number| number.name().to_owned())
    }
}

/// The types passed across the boundary. A parameter is borrowed for the
/// duration of the call and a return value is transferred, so `Str` is `&str`
/// as a parameter and `String` as a return value, `Bytes` is `&[u8]` and
/// `Vec<u8>`, and an interface is `&T` or `&mut T` as a parameter and `T` as
/// a return value. A shared interface may also be `Arc<T>` in either position.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    Void,
    Bool,
    I32,
    /// A Haxe `haxe.Int64`: allowed as a parameter, a return value, and in a
    /// future, stream or signal; not as a callback's parameter or return value.
    I64,
    F64,
    Str,
    Bytes,
    /// `&mut [u8]`: a Haxe `haxe.io.Bytes` the call writes into, borrowed for
    /// the call. Parameter type only.
    BytesMut,
    /// A handle to an interface value, which may be null when `nullable`.
    Handle {
        interface: String,
        nullable: bool,
    },
    /// An atomic cell both sides read and write: `&AtomicI32` as a parameter,
    /// borrowed for the call, and `Arc<AtomicI32>` as a return value, owned
    /// by the library.
    AtomicInt,
    /// The same for a flag: `&AtomicFlag` and `Arc<AtomicFlag>`.
    AtomicBool,
    /// `Future<T>`, a single asynchronous result. Return type only.
    Future(Box<Type>),
    /// `Stream<T>`, a sequence of results. Return type only.
    Stream(Box<Type>),
    /// `Signal<T>`, a value that changes over time. Return type only.
    Signal(Box<Type>),
    /// A Haxe callback, borrowed for the duration of the call:
    /// `impl Fn(..) -> Result<T, E>`. Parameter type only.
    Lent {
        params: Vec<Type>,
        returns: Box<Type>,
    },
    /// Plain data carried by a future, stream or signal, or as a callback's
    /// parameter or return value. A data parameter or return value of a
    /// function is instead its wire type, with the data type recorded beside
    /// it.
    Data(Data),
}

impl Library {
    /// Whether any function returns a future, stream or signal, which makes
    /// the adapter depend on `hxn-future`.
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

    /// The error types the library's functions fail with, in order of first
    /// use. Each is thrown to Haxe as its own exception class, and is
    /// identified on the wire by its index in this list.
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

    /// Whether any function returns an atomic cell to the host, which needs
    /// a library export to get the cell's address.
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
            Self::BytesMut => String::from("bytes_mut"),
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

/// A parse error, with its location.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Error {
    /// The file, when the library was read from files and not from text.
    pub file: Option<String>,
    /// 1-based, or zero when the error applies to the whole source.
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
