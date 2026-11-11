/// What an exported function returns. Zero is success, negative values are
/// hxn's own and positive values belong to the library.
///
/// A struct and not an enum, so a value hxn does not know is still a valid one.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Status(pub i32);

impl Status {
    pub const OK: Self = Self(0);
    /// The library failed and named no code of its own.
    pub const ERROR: Self = Self(1);
    /// The library panicked. Its state is whatever the panic left behind.
    pub const PANIC: Self = Self(-1);
    pub const NULL: Self = Self(-2);
    pub const INVALID_HANDLE: Self = Self(-3);
    pub const CLOSED: Self = Self(-4);
    pub const BUSY: Self = Self(-5);
    pub const ENCODING: Self = Self(-6);
    pub const TOO_LARGE: Self = Self(-7);
    /// A future's result was asked for before it had one.
    pub const PENDING: Self = Self(-8);
    /// A future lost everything that could settle it.
    pub const CANCELLED: Self = Self(-9);
    /// A host callback failed. The host has its own account of why, and
    /// reports that one once the call returns.
    pub const CALLBACK: Self = Self(-10);

    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }
}

/// A misuse of the boundary itself, as opposed to a failure of the library.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// A required pointer was null.
    Null,
    /// The pointer is not a handle, or is a handle to another type.
    InvalidHandle,
    /// The handle's value was already taken.
    Closed,
    /// The value is borrowed in a way that excludes this use: by another
    /// thread, or by a call further up this one's stack.
    Busy,
    /// Text that is not valid in its encoding.
    Encoding,
    /// A length no allocation can have.
    TooLarge,
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
        }
    }
}

/// Why a call failed: the status it returns and the message its caller reads.
///
/// Any `Display` error converts with `?` and reports [`Status::ERROR`]. It
/// implements neither `Display` nor `Error` itself, which is what leaves room
/// for that conversion.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    pub status: Status,
    pub message: String,
}

impl Failure {
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
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
