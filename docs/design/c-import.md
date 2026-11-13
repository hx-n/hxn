# Importing C libraries

[Design](../design.md) · [Conventions](../conventions.md)

`hxn import` creates a Rust crate with safe wrappers around a C library's API
and marks those wrappers for hxn generation. Both Rust programs and the
generated Haxe classes use this crate. `hxn-c` imports it in three steps:

1. clang reads the header and writes its AST as JSON
   (`-ast-dump=json`), so hxn needs clang installed but does not link
   libclang. Only declarations in the header itself are imported; included
   headers are read to resolve typedefs. The importer maps what it can:
   - A struct declared without its fields is an object. A function whose
     first parameter points to it is a method, named without the object's
     prefix (`tally_add` becomes `add`). A function named `free`, `destroy`
     and so on frees it, and one named `new`, `create` and so on that
     returns it is the constructor.
   - A struct with fields is passed by value if every field is a boolean or
     a number. An enum becomes a C-like `#[hxn::data]` enum.
   - `const char *` is text, and a `const` byte pointer followed by an
     integer length is one bytes parameter. A length other than a `size_t`
     is `[HxnLength]`, and bytes that do not fit it are an error before the
     call.
   - A non-`const` byte pointer followed by an integer length is bytes C
     writes into, `[HxnWritten]`: a `&mut [u8]` in Rust and a `haxe.io.Bytes`
     in Haxe that sees the writes. It is taken to be written only during the
     call, like a lent callback, and listed for checking. The length must be
     named as one (`n`, `len`, `size`, `count`, `*_len` and so on): a buffer
     C keeps, such as the one `sqlite3_deserialize` takes, comes with a size
     named otherwise.
   - A pointer to fixed-width integers, `float` or `double` followed by a
     count named the same way is an array: `sequence<double>`, a slice in
     Rust and an `Array` in Haxe, copied in for a `const` pointer. A
     non-`const` one is an array C fills, `[HxnOut]`: the caller passes how
     many elements and gets them back, and the C function returns nothing or
     a status. An integer whose width depends on the platform, `long` or
     `size_t`, is not an element, since Haxe's array cannot say which.
   - A `void (*)(void *)` after a text or bytes parameter is taken as its
     destructor, `[HxnFrees]`, as in `sqlite3_bind_text`. C is given a copy
     and a destructor that frees the copy, so it may keep the text or bytes
     for as long as it wants. Any other function pointer passed without user
     data is not imported; one that C is always passed the same value for,
     such as `SQLITE_TRANSIENT`, is added by hand with `[HxnPass=n]`. A returned `char *` is freed with the library's
     only function that takes a `char *` and frees it, if it has exactly
     one.
   - A non-`const` pointer to a number, an enum, a struct, text or an
     object pointer after all the passed parameters is an output: the
     function returns what C writes through it, and an `int` return value
     becomes a status. Outputs after the first are passed `NULL`. A function
     that writes an object through a pointer is that object's constructor
     or static function, so `sqlite3_open(path, &db)` becomes the
     constructor of `Sqlite3`.
   - An object returned by a method of another object is taken as borrowed:
     it belongs to that object, and is not freed by Haxe.
   - A function pointer followed by a `void *` for user data is a callback,
     declared as a `callback` WebIDL definition of its type. It takes
     booleans, integers, floating-point numbers, strings, enums or structs,
     and returns nothing, a boolean or an `int`. A callback is lent for the
     call: a Haxe function that takes at most four values, all `int`s and
     not other integers. A function whose name suggests it keeps the
     callback to call later (`set`, `register`, `hook`, `handler` and so
     on) keeps it instead, `[HxnKept]`: an `int` it returns is taken as a
     status, and a pointer, usually the callback it replaced, is ignored.
   - Each of these guesses is listed under "Check:" at the end of the
     WebIDL, and anything that cannot be mapped is listed as not imported.
   - A header may declare functions the installed library leaves out, as
     sqlite3's does for its optional features, and a binding that refers to
     one does not load. For each library given to `--link` that the C
     compiler finds as a file, `nm` lists what it defines, and a function it
     does not define is listed as not imported.
2. Review the generated WebIDL and add ownership, nullability and status
   information through extended attributes: `[HxnStatus]` makes a
   non-zero return value an exception, `[HxnFree=f]` names the function
   that frees an object or returned text, `[HxnOut]` and `[HxnNull]` mark
   outputs, `[HxnBorrowed]` a returned object the caller must not free,
   `[HxnThreadSafe]` an object the library allows calls on from several
   threads at once, `[HxnUser]` where C passes a callback its user data,
   `[HxnStop]` what C is returned when the Haxe function throws,
   `[HxnKept]` a callback C keeps, and `[HxnPass]` a pointer C is always
   passed the same value for (see
   `hxn-c`'s `idl` module for the full list). The file is committed. `hxn-c` reads only
   this subset of WebIDL, and reports the line of anything else.
3. Each build regenerates `src/bindings.rs` from the reviewed WebIDL: `extern "C"` declarations, and safe wrappers over them. An
   object owns its C pointer and frees it in `Drop`, unless it is borrowed.
   An object C writes through a pointer is owned before the status is
   checked, so it is freed even when the call fails. Because a C library is
   not assumed to be thread-safe, every method takes `&mut self`, so the
   handle allows one call at a time, and the object is only `Send`; a
   `[HxnThreadSafe]` object's methods take `&self` and it is `Sync`. A
   wrapper returns `Result<_, String>` where the call can fail: text with a
   NUL, a platform-sized integer that does not fit, a C enum value with no
   variant, a null object pointer, or a status code.
   A callback is passed to C as a generated `extern "C"` trampoline, with
   the Haxe function as its user data. C cannot be unwound through, so the
   trampoline catches a panic and keeps it, keeps a failure, and returns
   `[HxnStop]` to C, or does nothing when C cannot be told to stop. Once C
   returns, the panic is resumed or the Haxe function's failure returned,
   ahead of anything the call itself reports.
   For a retained C callback, the registration function returns a `Stream`.
   The generated trampoline
   copies what C passes, sends it to the stream, and returns `[HxnStop]` to
   C on every call; the program reads the calls on its own thread. With more
   than one value, each call is a record, `<Callback>Event`. The stream's
   sender is C's user data. A method's object owns it and drops it after its
   C free function runs, when C can no longer call the callback, and the
   stream ends. A sender given to a function with no object, or to a
   borrowed object, is never freed, since nothing reports when C stops
   calling it. A callback registered again replaces the earlier one in C,
   and the earlier stream ends when the object is freed.

The crate's `build.rs` compiles the C sources given to `--source` with the
`cc` crate, or links the libraries given to `--link`. For hxcpp, the build
reads system-library dependencies from Cargo's `linked_libs` and
`linked_paths` messages and writes them to `link.xml`. The generated glue
includes this file, and packages keep it beside the archive. A static archive
requires these separate link settings for its system-library dependencies. The
same process applies to imported libraries and other crates that link system
libraries.

The generated build XML lists `hxn.h` and the library's two glue headers as
dependencies of every compiled file. Header changes therefore recompile the
affected objects, compensating for hxcpp's timestamp-only rebuild check. For
`cpp --wasm`, hxn tells `cc` to compile with `emcc`. `hl --wasm` is not
supported, because compiling C for `wasm32-wasip1` needs the WASI SDK.

For a C++ library, provide a C API that specifies how its classes, overloads,
templates and exceptions are exposed. `hxn import` reads that C API; the
library author defines the C++ conversion.

The importer lists these unsupported declarations as not imported:

- Objects passed to callbacks remain owned by C, which may free them when
  the callback returns. A Haxe function could retain an object beyond that
  lifetime, so these arguments cannot use hxn's ownership-transfer protocol.
- A callback whose user data is neither first nor last, or that takes
  other untyped pointers, as `sqlite3_trace_v2`'s does: what those point to
  depends on the other arguments.
- Variadic functions and functions that use `va_list` are unsupported.
- Pointers whose meaning the header does not give, such as a `void *`
  with no length, or a buffer C keeps. A function that needs one is
  written in the library's own Rust beside the generated bindings.

See also: [Testing](../conventions/testing.md).
