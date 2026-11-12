/// The return value of an exported function. Zero is success, negative values
/// are reserved for hxn, and positive values are defined by the library.
///
/// A struct rather than an enum, so that a code hxn does not define is still a
/// valid value.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Status(pub i32);

impl Status {
    pub const OK: Self = Self(0);
    /// The library failed without specifying a code of its own.
    pub const ERROR: Self = Self(1);
    /// The library panicked. Its state is whatever the panic left behind.
    pub const PANIC: Self = Self(-1);
    pub const NULL: Self = Self(-2);
    pub const INVALID_HANDLE: Self = Self(-3);
    pub const CLOSED: Self = Self(-4);
    pub const BUSY: Self = Self(-5);
    pub const ENCODING: Self = Self(-6);
    pub const TOO_LARGE: Self = Self(-7);
    /// A future's result was requested before the future was settled.
    pub const PENDING: Self = Self(-8);
    /// Everything that could settle the future was dropped.
    pub const CANCELLED: Self = Self(-9);
    /// A host callback failed. The host has recorded the cause and reports it
    /// once the call returns.
    pub const CALLBACK: Self = Self(-10);
    pub const RANGE: Self = Self(-11);
    /// Recursive data nested deeper than the reader accepts.
    pub const DEPTH: Self = Self(-12);

    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }
}

/// An incorrect use of the boundary, as opposed to a failure in the library.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// A required pointer was null.
    Null,
    /// The pointer is not a handle, or is a handle to another type.
    InvalidHandle,
    /// The handle's value was already taken.
    Closed,
    /// The value has a borrow that conflicts with this use, held by another
    /// thread or by a call further up this thread's stack.
    Busy,
    /// Text that is not valid in its encoding.
    Encoding,
    /// A length larger than any allocation can be.
    TooLarge,
    /// A number out of range for its target type.
    Range,
    /// Data nested deeper than the reader accepts.
    Depth,
}

impl Fault {
    pub const fn status(self) -> Status {
        match self {
            Self::Null => Status::NULL,
            Self::InvalidHandle => Status::INVALID_HANDLE,
            Self::Closed => Status::CLOSED,
            Self::Busy => Status::BUSY,
            Self::Encoding => Status::ENCODING,
            Self::TooLarge => Status::TOO_LARGE,
            Self::Range => Status::RANGE,
            Self::Depth => Status::DEPTH,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::Null => "null pointer",
            Self::InvalidHandle => "not a handle of this type",
            Self::Closed => "handle is closed",
            Self::Busy => "handle is in use",
            Self::Encoding => "invalid text encoding",
            Self::TooLarge => "length out of range",
            Self::Range => "number out of range",
            Self::Depth => "data nests too deep",
        }
    }
}

/// The reason a call failed: the status it returns and the message for its
/// caller.
///
/// Any `Display` error converts to it with `?` and reports [`Status::ERROR`].
/// `Failure` implements neither `Display` nor `Error`, because either would
/// conflict with that blanket conversion.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    pub status: Status,
    pub message: String,
    /// The library's error serialized as plain data, for a host that throws it
    /// as a typed error. Empty when the failure has only a message.
    pub value: Vec<u8>,
}

impl Failure {
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            value: Vec::new(),
        }
    }

    /// Returns the same failure with `value` attached.
    pub fn with_value(mut self, value: Vec<u8>) -> Self {
        self.value = value;
        self
    }
}

impl From<Fault> for Failure {
    fn from(fault: Fault) -> Self {
        Self::new(fault.status(), fault.message())
    }
}

impl<E: std::fmt::Display> From<E> for Failure {
    fn from(error: E) -> Self {
        Self::new(Status::ERROR, error.to_string())
    }
}
