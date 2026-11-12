//! A C library's API as the WebIDL describes it.

/// Everything imported from one header.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Api {
    /// The library's name: the Rust crate's and the Haxe class for its free
    /// functions.
    pub library: String,
    pub enums: Vec<Enum>,
    pub structs: Vec<Struct>,
    pub objects: Vec<Object>,
    /// Functions that belong to no object.
    pub functions: Vec<Function>,
    /// The kinds of function pointer C is passed, with user data.
    pub callbacks: Vec<Callback>,
    /// Declarations the importer could not map, each with the reason. Written
    /// to the WebIDL as comments.
    pub skipped: Vec<String>,
    /// Guesses the importer made that need checking. Written to the WebIDL
    /// as comments.
    pub review: Vec<String>,
}

/// A C enum: a Haxe `enum abstract` over `Int`.
#[derive(Clone, PartialEq, Debug)]
pub struct Enum {
    pub name: String,
    pub docs: Vec<String>,
    pub values: Vec<(String, i64)>,
}

/// A C function pointer type, passed with a `void *` for user data: a Haxe
/// function lent for the call.
#[derive(Clone, PartialEq, Debug)]
pub struct Callback {
    pub name: String,
    pub docs: Vec<String>,
    /// What the function takes, less the user data.
    pub params: Vec<Param>,
    pub returns: Ty,
    /// Whether C passes the user data first; otherwise last.
    pub user_first: bool,
    /// `[HxnStop=n]`: what C is returned when the Haxe function throws, and
    /// every time for a callback C keeps, since Haxe answers later.
    pub stop: Option<i64>,
}

/// A C struct passed by value: a Haxe class with the same fields.
#[derive(Clone, PartialEq, Debug)]
pub struct Struct {
    pub name: String,
    pub docs: Vec<String>,
    pub fields: Vec<Param>,
}

/// An opaque C type used through a pointer: a Haxe class that owns one.
#[derive(Clone, PartialEq, Debug)]
pub struct Object {
    pub name: String,
    pub docs: Vec<String>,
    /// The C function that frees a pointer to it. Without one, the binding
    /// never frees it.
    pub free: Option<String>,
    /// The free function returns a C `int`, which is ignored.
    pub free_int: bool,
    /// `[HxnThreadSafe]`: the C library allows calls on one object from
    /// several threads at once, so methods take `&self`.
    pub thread_safe: bool,
    pub constructor: Option<Function>,
    /// Functions whose first C parameter is the object. `params` leaves it
    /// out.
    pub methods: Vec<Function>,
    pub statics: Vec<Function>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Function {
    /// The name in WebIDL, Rust and Haxe.
    pub name: String,
    /// The C function it calls.
    pub c: String,
    pub docs: Vec<String>,
    pub params: Vec<Param>,
    pub returns: Ty,
    pub status: Status,
    /// The C function that frees the returned text after it is copied.
    pub free: Option<String>,
    /// `[HxnBorrowed]`: the returned object belongs to something else, and
    /// is not freed by its wrapper.
    pub borrowed: bool,
    /// The C return value of a function that keeps a callback.
    pub ignored: Ignored,
}

/// How a C function reports failure through its return value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// It does not: the return value is the result.
    None,
    /// `[HxnStatus]`: 0 is success and any other value a failure. The
    /// function returns nothing.
    Zero,
    /// `[HxnStatus=negative]`: a negative value is a failure, and any other
    /// value is the result.
    Negative,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    pub role: Role,
}

/// How a parameter is passed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Role {
    /// The caller passes it.
    #[default]
    In,
    /// `[HxnOut]`: C writes it through a pointer, and the function returns
    /// it. The caller does not pass it.
    Out,
    /// `[HxnNull]`: an output nobody reads. C is passed a null pointer.
    Null,
    /// `[HxnKept]`: a callback C keeps, to call after the function returns.
    /// What it is called with is queued for Haxe as a stream, which the
    /// function returns.
    Kept,
    /// `[HxnPass=n]`: C is always passed `n` as a pointer, such as a
    /// destructor constant like sqlite3's `SQLITE_TRANSIENT`. The caller
    /// does not pass it.
    Pass(i64),
    /// `[HxnWritten]`: bytes C writes into during the call, which the caller
    /// passes and sees the writes in.
    Written,
    /// `[HxnOut]` on a `sequence`: an array C fills, as many elements as the
    /// caller asks for, which the function returns.
    Filled,
    /// `[HxnFrees]`: a `void (*)(void *)` C calls with the text or bytes
    /// argument before it once it no longer needs them. C is given a copy
    /// and a function that frees it. The caller does not pass it.
    Frees,
}

/// A C return value the binding ignores: a registration function's, which
/// reports the callback it replaced or a status nothing reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Ignored {
    #[default]
    None,
    /// `[HxnIgnore=int]`.
    Int,
    /// `[HxnIgnore=pointer]`.
    Pointer,
}

impl Role {
    /// Whether the caller passes the argument.
    pub fn passed(self) -> bool {
        matches!(self, Self::In | Self::Written)
    }
}

impl Param {
    pub fn new(name: impl Into<String>, ty: Ty) -> Self {
        Self {
            name: name.into(),
            ty,
            role: Role::In,
        }
    }
}

impl Api {
    /// Leaves out every function whose C function `exported` reports the
    /// library does not define, and an object's free function likewise.
    /// Each is added to `skipped` with `why`.
    pub fn retain(&mut self, exported: impl Fn(&str) -> bool, why: &str) {
        let mut skipped = Vec::new();
        let mut keep = |function: &Function| {
            let kept = exported(&function.c);
            if !kept {
                skipped.push(format!("{}: {why}", function.c));
            }
            kept
        };
        self.functions.retain(&mut keep);
        for object in &mut self.objects {
            object.methods.retain(&mut keep);
            object.statics.retain(&mut keep);
            if object.constructor.as_ref().is_some_and(|made| !keep(made)) {
                object.constructor = None;
            }
        }
        for object in &mut self.objects {
            if let Some(free) = &object.free
                && !exported(free)
            {
                skipped.push(format!("{free}: {why}; `{}` is not freed", object.name));
                object.free = None;
            }
        }
        self.skipped.extend(skipped);
    }
}

impl Ty {
    /// The object it is, or may be.
    pub fn inner_object(&self) -> Option<&str> {
        match self {
            Self::Object(name) => Some(name),
            Self::Nullable(of) => of.inner_object(),
            _ => None,
        }
    }
}

/// The text or bytes argument the `[HxnFrees]` argument at `at` frees: the
/// nearest one before it.
pub fn freed(params: &[Param], at: usize) -> Option<usize> {
    params[..at]
        .iter()
        .rposition(|p| p.role == Role::In && matches!(p.ty, Ty::Text | Ty::Bytes(_)))
}

impl Function {
    /// The `[HxnOut]` parameter, if it has one.
    pub fn out(&self) -> Option<&Param> {
        self.params.iter().find(|param| param.role == Role::Out)
    }

    /// The array C fills, if it has one.
    pub fn filled(&self) -> Option<&Param> {
        self.params.iter().find(|param| param.role == Role::Filled)
    }

    /// The `[HxnKept]` callback, if it has one.
    pub fn kept(&self) -> Option<&Param> {
        self.params.iter().find(|param| param.role == Role::Kept)
    }
}

/// An integer type, with the C types whose width depends on the platform
/// kept apart from the fixed-width ones.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Int {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    /// C `long`: 32 bits on Windows, 64 elsewhere. Passed as an `i64`.
    Long,
    /// C `unsigned long`. Passed as a `u64`.
    ULong,
    /// `size_t`. Passed as a `u64`.
    Size,
    /// `ssize_t` or `ptrdiff_t`. Passed as an `i64`.
    SSize,
}

impl Int {
    /// Whether it has the same width everywhere, which an array element
    /// needs.
    pub fn fixed(self) -> bool {
        !matches!(self, Self::Long | Self::ULong | Self::Size | Self::SSize)
    }

    /// The C type, as `[HxnLength=...]` names it.
    pub fn c_name(self) -> &'static str {
        match self {
            Self::I8 => "int8_t",
            Self::U8 => "uint8_t",
            Self::I16 => "int16_t",
            Self::U16 => "uint16_t",
            Self::I32 => "int",
            Self::U32 => "unsigned",
            Self::I64 => "int64_t",
            Self::U64 => "uint64_t",
            Self::Long => "long",
            Self::ULong => "unsigned_long",
            Self::Size => "size_t",
            Self::SSize => "ssize_t",
        }
    }

    /// The integer type `[HxnLength=...]` names.
    pub fn from_c_name(name: &str) -> Option<Self> {
        [
            Self::I8,
            Self::U8,
            Self::I16,
            Self::U16,
            Self::I32,
            Self::U32,
            Self::I64,
            Self::U64,
            Self::Long,
            Self::ULong,
            Self::Size,
            Self::SSize,
        ]
        .into_iter()
        .find(|int| int.c_name() == name)
    }

    /// The platform-dependent C type, as `[HxnC=...]` names it.
    pub fn alias(self) -> Option<&'static str> {
        match self {
            Self::Long => Some("long"),
            Self::ULong => Some("unsigned_long"),
            Self::Size => Some("size_t"),
            Self::SSize => Some("ssize_t"),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Void,
    Bool,
    Int(Int),
    F32,
    F64,
    /// `const char *`: UTF-8 text ending in a NUL.
    Text,
    /// A `const uint8_t *` and its length, of the integer type given, as one
    /// parameter: a `size_t` unless `[HxnLength]` names another.
    Bytes(Int),
    Enum(String),
    /// A struct passed by value.
    Struct(String),
    /// A struct passed as a `const` pointer.
    StructRef(String),
    /// A pointer to an object. As a return value, a null pointer is a
    /// failure.
    Object(String),
    /// A pointer that may be null: text or an object.
    Nullable(Box<Ty>),
    /// A function pointer and the user data after it, as one parameter.
    Callback(String),
    /// A pointer of no type C is given, which is always `[HxnPass]`.
    Pointer,
    /// A pointer to numbers and their count, of the integer type given, as
    /// one parameter: a fixed-width integer, `float` or `double`.
    Array(Box<Ty>, Int),
}
