//! Parses a library's source into its [`Library`]: the marked items,
//! rejecting anything the emitters cannot support.

use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Attribute, FnArg, ImplItem, Item, Meta, ReturnType, Visibility};

use crate::model::{
    Case, Data, Enumeration, Error, Field, Interface, Library, Method, Number, Param, Receiver,
    Record, Tagged, Type, Variant,
};
use crate::names::{exception_class, pascal, wire_class};

fn error<T>(span: Span, message: impl Into<String>) -> Result<T, Error> {
    Err(Error {
        file: None,
        line: span.start().line,
        message: message.into(),
    })
}

// Whether `attribute` is `#[hxn::<name>]`, such as `#[hxn::interface]` or
// `#[hxn::methods]`.
fn is_marker(attribute: &Attribute, name: &str) -> bool {
    let segments: Vec<_> = attribute
        .path()
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    segments == ["hxn", name]
}

/// What `#[hxn::interface(...)]` specifies for a struct.
#[derive(Default)]
struct Notes {
    shared: bool,
    weighed: bool,
}

fn notes(attributes: &[Attribute]) -> Result<Notes, Error> {
    let mut notes = Notes::default();
    for attribute in attributes.iter().filter(|a| is_marker(a, "interface")) {
        match &attribute.meta {
            Meta::Path(_) => {}
            Meta::List(_) => {
                let mut unknown = None;
                let parsed = attribute.parse_nested_meta(|note| {
                    if note.path.is_ident("shared") {
                        notes.shared = true;
                    } else if note.path.is_ident("weighed") {
                        notes.weighed = true;
                    } else {
                        unknown = note.path.get_ident().map(ToString::to_string);
                        return Err(note.error("unknown note"));
                    }
                    Ok(())
                });
                if parsed.is_err() {
                    return error(
                        attribute.span(),
                        match unknown {
                            Some(note) => format!("unknown note `{note}`"),
                            None => String::from("expected `#[hxn::interface(...)]`"),
                        },
                    );
                }
            }
            Meta::NameValue(_) => {
                return error(attribute.span(), "expected `#[hxn::interface(...)]`");
            }
        }
    }
    Ok(notes)
}

fn docs(attributes: &[Attribute]) -> Vec<String> {
    attributes
        .iter()
        .filter_map(|attribute| match &attribute.meta {
            Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(text),
                    ..
                }) => Some(text.value().trim().to_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

// The `#[hxn(...)]` attribute on a function. `blocking` is the only option.
fn is_blocking(attributes: &[Attribute]) -> Result<bool, Error> {
    let mut blocking = false;
    for attribute in attributes {
        if !attribute.path().is_ident("hxn") {
            continue;
        }
        let Meta::List(list) = &attribute.meta else {
            return error(attribute.span(), "expected `#[hxn(...)]`");
        };
        let note = list.tokens.to_string();
        if note != "blocking" {
            return error(attribute.span(), format!("unknown note `{note}`"));
        }
        blocking = true;
    }
    Ok(blocking)
}

fn path_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => {
            let segment = path.path.segments.last()?;
            (path.path.segments.len() == 1 && segment.arguments.is_none())
                .then(|| segment.ident.to_string())
        }
        _ => None,
    }
}

// Unwraps `Option<T>` to `T`.
fn optional(ty: &syn::Type) -> Option<&syn::Type> {
    wrapped(ty, "Option")
}

// The atomic cell type `ty` names: `AtomicI32` or `AtomicFlag`.
fn cell(ty: &syn::Type) -> Option<Type> {
    match path_name(ty)?.as_str() {
        "AtomicI32" => Some(Type::AtomicInt),
        "AtomicFlag" => Some(Type::AtomicBool),
        _ => None,
    }
}

// Unwraps `wrapper<T, ..>` to `T`.
fn wrapped_first<'a>(ty: &'a syn::Type, wrapper: &str) -> Option<&'a syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.first() {
        Some(syn::GenericArgument::Type(inner)) => Some(inner),
        _ => None,
    }
}

// Unwraps `wrapper<T>` to `T`.
fn wrapped<'a>(ty: &'a syn::Type, wrapper: &str) -> Option<&'a syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if path.path.segments.len() != 1 || segment.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.first() {
        Some(syn::GenericArgument::Type(inner)) if arguments.args.len() == 1 => Some(inner),
        _ => None,
    }
}

/// The interfaces a library marked, and the one whose functions are being read.
struct Scope<'a> {
    interfaces: &'a [String],
    // Those of them marked `shared`.
    shared: &'a [String],
    own: &'a str,
    // The names of the library's `#[hxn::data]` structs and enums.
    known: Known<'a>,
}

// Unwraps `HashMap<K, V>` or `BTreeMap<K, V>` to `K`, `V` and whether it is
// the sorted one.
fn mapped(ty: &syn::Type) -> Option<(&syn::Type, &syn::Type, bool)> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let sorted = match segment.ident.to_string().as_str() {
        "HashMap" => false,
        "BTreeMap" => true,
        _ => return None,
    };
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter();
    match (types.next(), types.next(), types.next()) {
        (Some(syn::GenericArgument::Type(key)), Some(syn::GenericArgument::Type(value)), None) => {
            Some((key, value, sorted))
        }
        _ => None,
    }
}

// Unwraps a set to its element and whether it is ordered.
fn set(ty: &syn::Type) -> Option<(&syn::Type, bool)> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let sorted = match segment.ident.to_string().as_str() {
        "HashSet" => false,
        "BTreeSet" => true,
        _ => return None,
    };
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter();
    match (types.next(), types.next()) {
        (Some(syn::GenericArgument::Type(of)), None) => Some((of, sorted)),
        _ => None,
    }
}

// Plain data, given the names of the library's data types. `None` for a type
// that is not data.
fn data_of(ty: &syn::Type, known: Known) -> Option<Data> {
    if let syn::Type::Tuple(tuple) = ty {
        return if tuple.elems.is_empty() {
            None
        } else {
            tuple
                .elems
                .iter()
                .map(|ty| data_of(ty, known))
                .collect::<Option<Vec<_>>>()
                .map(Data::Tuple)
        };
    }
    if let Some(inner) = optional(ty) {
        return Some(Data::Option(Box::new(data_of(inner, known)?)));
    }
    if let Some(inner) = wrapped(ty, "Vec") {
        if path_name(inner).as_deref() == Some("u8") {
            return Some(Data::Bytes);
        }
        return Some(Data::List(Box::new(data_of(inner, known)?)));
    }
    if let Some(inner) = wrapped(ty, "Box") {
        return Some(Data::Boxed(Box::new(data_of(inner, known)?)));
    }
    if let Some((of, sorted)) = set(ty) {
        return Some(Data::Set {
            of: Box::new(data_of(of, known)?),
            sorted,
        });
    }
    if let Some((key, value, sorted)) = mapped(ty) {
        // The key types a Haxe `Map` supports.
        let key = data_of(key, known).filter(|key| {
            matches!(
                key,
                Data::Text
                    | Data::I32
                    | Data::Number(Number::I8 | Number::I16 | Number::U8 | Number::U16)
                    | Data::Enum(_)
            )
        })?;
        return Some(Data::Map {
            key: Box::new(key),
            value: Box::new(data_of(value, known)?),
            sorted,
        });
    }
    let name = path_name(ty)?;
    let named = |names: &[String]| names.contains(&name);
    Some(match name.as_str() {
        "bool" => Data::Bool,
        "i32" => Data::I32,
        "i64" => Data::I64,
        "f64" => Data::F64,
        "String" => Data::Text,
        number if Number::parse(number).is_some() => Data::Number(Number::parse(number)?),
        _ if named(known.records) => Data::Record(name),
        _ if named(known.enums) => Data::Enum(name),
        _ if named(known.tagged) => Data::Tagged(name),
        _ => return None,
    })
}

impl Scope<'_> {
    // The interface `ty` names, with `Self` as the one being read.
    fn interface(&self, ty: &syn::Type) -> Option<String> {
        let name = path_name(ty)?;
        if name == "Self" {
            Some(self.own.to_owned())
        } else {
            self.interfaces.contains(&name).then_some(name)
        }
    }

    // `&T` or `&mut T` of an interface, and whether it is the latter.
    fn borrowed(&self, ty: &syn::Type) -> Option<(String, bool)> {
        let syn::Type::Reference(reference) = ty else {
            return None;
        };
        Some((
            self.interface(&reference.elem)?,
            reference.mutability.is_some(),
        ))
    }

    // Unwraps `Arc<T>` of an interface to `T`.
    fn pointer(&self, ty: &syn::Type) -> Option<String> {
        self.interface(wrapped(ty, "Arc")?)
    }

    // The element of a `Vec` of an interface, `Vec<T>`, or of a shared one,
    // `Vec<Arc<T>>`.
    fn listed(&self, ty: &syn::Type) -> Result<Option<Type>, Error> {
        let Some(element) = wrapped(ty, "Vec") else {
            return Ok(None);
        };
        let (interface, arc) = match (self.interface(element), self.pointer(element)) {
            (Some(interface), _) => (interface, false),
            (None, Some(interface)) => (interface, true),
            (None, None) => return Ok(None),
        };
        if self.is_shared(&interface) != arc {
            return error(
                ty.span(),
                if arc {
                    format!(
                        "`Arc<{interface}>` needs `{interface}` marked `#[hxn::interface(shared)]`"
                    )
                } else {
                    format!("a shared interface is listed in its `Arc`: `Vec<Arc<{interface}>>`")
                },
            );
        }
        Ok(Some(Type::Handle {
            interface,
            nullable: false,
        }))
    }

    fn is_shared(&self, interface: &str) -> bool {
        self.shared.iter().any(|shared| shared == interface)
    }

    // Data that needs its own wire type: an enum, a record, or an `Option`
    // or a list of data. A type the boundary already supports, a bare number
    // or text, is `None` here.
    fn shape(&self, ty: &syn::Type) -> Option<Data> {
        data_of(ty, self.known).filter(|data| data.wire().is_some())
    }

    // The same behind a `&`, where a list may also be a slice.
    fn lent_shape(&self, ty: &syn::Type) -> Option<Data> {
        match ty {
            syn::Type::Slice(slice) => {
                data_of(&slice.elem, self.known).map(|of| Data::List(Box::new(of)))
            }
            other => self.shape(other),
        }
    }

    // `impl Fn(i32, f64) -> Result<bool, E>`, or `FnMut` or `FnOnce`.
    fn lent(&self, ty: &syn::TypeImplTrait) -> Result<Type, Error> {
        const SHAPE: &str = "a function is lent as `impl Fn(..) -> Result<T, E>`, taking bool, i32, i64, f64, &str, &[u8], data marked `#[hxn::data]` or an object to keep, and giving bool, i32, i64, f64, String, Vec<u8>, data or `()`: it can fail, because the Haxe function can throw";
        let scalar = |ty: &syn::Type| match path_name(ty).as_deref() {
            Some("bool") => Some(Type::Bool),
            Some("i32") => Some(Type::I32),
            // Keep all 64 bits through runtimes whose dynamic numeric calls
            // otherwise pass them as a floating-point value.
            Some("i64") => Some(Type::Data(Data::I64)),
            Some("f64") => Some(Type::F64),
            _ => None,
        };
        // Text and bytes passed to the callback are borrowed. An object
        // passed to it is transferred: a borrowed one could be kept past the
        // call.
        let taken = |ty: &syn::Type| -> Result<Option<Type>, Error> {
            if let syn::Type::Reference(reference) = ty {
                if reference.mutability.is_none() {
                    if path_name(&reference.elem).as_deref() == Some("str") {
                        return Ok(Some(Type::Str));
                    }
                    if let syn::Type::Slice(slice) = &*reference.elem
                        && path_name(&slice.elem).as_deref() == Some("u8")
                    {
                        return Ok(Some(Type::Bytes));
                    }
                    // Data is copied for the callback either way.
                    if let Some(data) = self.lent_shape(&reference.elem) {
                        return Ok(Some(Type::Data(data)));
                    }
                }
                if self.interface(&reference.elem).is_some() {
                    return error(
                        ty.span(),
                        "a lent function is given an object to keep, not one to borrow: pass `T`, or `Arc<T>` of a shared interface",
                    );
                }
                return Ok(None);
            }
            if let Some(interface) = self.interface(ty) {
                if self.is_shared(&interface) {
                    return error(
                        ty.span(),
                        format!("a shared interface is given in its `Arc`: `Arc<{interface}>`"),
                    );
                }
                return Ok(Some(Type::Handle {
                    interface,
                    nullable: false,
                }));
            }
            if let Some(interface) = self.pointer(ty) {
                if !self.is_shared(&interface) {
                    return error(
                        ty.span(),
                        format!(
                            "`Arc<{interface}>` needs `{interface}` marked `#[hxn::interface(shared)]`"
                        ),
                    );
                }
                return Ok(Some(Type::Handle {
                    interface,
                    nullable: false,
                }));
            }
            if let Some(data) = self.shape(ty) {
                return Ok(Some(Type::Data(data)));
            }
            Ok(scalar(ty))
        };
        let mut bounds = ty.bounds.iter();
        let (Some(syn::TypeParamBound::Trait(bound)), None) = (bounds.next(), bounds.next()) else {
            return error(ty.span(), SHAPE);
        };
        let Some(segment) = bound.path.segments.last() else {
            return error(ty.span(), SHAPE);
        };
        let syn::PathArguments::Parenthesized(call) = &segment.arguments else {
            return error(ty.span(), SHAPE);
        };
        if !["Fn", "FnMut", "FnOnce"].contains(&segment.ident.to_string().as_str())
            || call.inputs.len() > 4
        {
            return error(ty.span(), SHAPE);
        }
        let mut params = Vec::new();
        for input in &call.inputs {
            match taken(input)? {
                Some(param) => params.push(param),
                None => return error(input.span(), SHAPE),
            }
        }
        let ReturnType::Type(_, output) = &call.output else {
            return error(ty.span(), SHAPE);
        };
        let Some(given) = wrapped_first(output, "Result") else {
            return error(output.span(), SHAPE);
        };
        let returns = match given {
            syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Type::Void,
            other if path_name(other).as_deref() == Some("String") => Type::Str,
            other if wrapped(other, "Vec").and_then(path_name).as_deref() == Some("u8") => {
                Type::Bytes
            }
            other => match (self.shape(other), scalar(other)) {
                (Some(data), _) => Type::Data(data),
                (None, Some(given)) => given,
                (None, None) => return error(other.span(), SHAPE),
            },
        };
        Ok(Type::Lent {
            params,
            returns: Box::new(returns),
        })
    }

    fn param(&self, name: String, ty: &syn::Type) -> Result<Param, Error> {
        let plain = |ty| Param {
            name: name.clone(),
            ty,
            exclusive: false,
            kept: false,
            taken: false,
            number: None,
            data: None,
            borrowed: false,
        };
        if let syn::Type::ImplTrait(lent) = ty {
            return Ok(plain(self.lent(lent)?));
        }
        if let syn::Type::Reference(reference) = ty
            && reference.mutability.is_none()
        {
            if path_name(&reference.elem).as_deref() == Some("str") {
                return Ok(plain(Type::Str));
            }
            if let syn::Type::Slice(slice) = &*reference.elem
                && path_name(&slice.elem).as_deref() == Some("u8")
            {
                return Ok(plain(Type::Bytes));
            }
            if let Some(cell) = cell(&reference.elem) {
                return Ok(plain(cell));
            }
        }
        // Bytes the call writes into: the Haxe `Bytes` sees what was written.
        if let syn::Type::Reference(reference) = ty
            && reference.mutability.is_some()
            && let syn::Type::Slice(slice) = &*reference.elem
            && path_name(&slice.elem).as_deref() == Some("u8")
        {
            return Ok(plain(Type::BytesMut));
        }
        if cell(ty).is_some() || wrapped(ty, "Arc").and_then(cell).is_some() {
            return error(
                ty.span(),
                "a cell is lent for the call: take `&AtomicI32` or `&AtomicFlag`",
            );
        }
        let (inner, nullable) = match optional(ty) {
            Some(inner) => (inner, true),
            None => (ty, false),
        };
        if let Some((interface, exclusive)) = self.borrowed(inner) {
            if exclusive && self.is_shared(&interface) {
                return error(
                    ty.span(),
                    "a shared interface is reached from more than one place: take `&T`, and change it through a cell or a lock inside",
                );
            }
            return Ok(Param {
                name,
                ty: Type::Handle {
                    interface,
                    nullable,
                },
                exclusive,
                kept: false,
                taken: false,
                number: None,
                data: None,
                borrowed: false,
            });
        }
        if let Some(interface) = self.pointer(inner) {
            if !self.is_shared(&interface) {
                return error(
                    ty.span(),
                    format!(
                        "`Arc<{interface}>` needs `{interface}` marked `#[hxn::interface(shared)]`"
                    ),
                );
            }
            return Ok(Param {
                name,
                ty: Type::Handle {
                    interface,
                    nullable,
                },
                exclusive: false,
                kept: true,
                taken: false,
                number: None,
                data: None,
                borrowed: false,
            });
        }
        if let syn::Type::Reference(reference) = inner
            && self.pointer(&reference.elem).is_some()
        {
            return error(
                ty.span(),
                "a shared interface is taken as `&T` to use it, or `Arc<T>` to keep a share",
            );
        }
        // Taken by value: moved out of its handle.
        if let Some(interface) = self.interface(inner) {
            if self.is_shared(&interface) {
                return error(
                    ty.span(),
                    format!(
                        "a shared interface is kept as `Arc<{interface}>`: its value has other holders"
                    ),
                );
            }
            return Ok(Param {
                name,
                ty: Type::Handle {
                    interface,
                    nullable,
                },
                exclusive: false,
                kept: false,
                taken: true,
                number: None,
                data: None,
                borrowed: false,
            });
        }
        // Data is copied in either way: passed by value, or by a reference
        // to the adapter's copy. A list may also be accepted as a slice.
        let (target, borrowed) = match ty {
            syn::Type::Reference(reference) if reference.mutability.is_none() => {
                (&*reference.elem, true)
            }
            other => (other, false),
        };
        let shape = if borrowed {
            self.lent_shape(target)
        } else {
            self.shape(target)
        };
        if let Some(data) = shape {
            return Ok(Param {
                borrowed,
                ..plain(data.wire().unwrap_or(Type::Bytes))
            }
            .of(data));
        }
        match path_name(ty).as_deref() {
            Some("bool") => Ok(plain(Type::Bool)),
            Some("i32") => Ok(plain(Type::I32)),
            Some("i64") => Ok(plain(Type::I64)),
            Some("f64") => Ok(plain(Type::F64)),
            Some(name) if Number::parse(name).is_some() => {
                let number = Number::parse(name);
                Ok(Param {
                    number,
                    ..plain(number.map_or(Type::I32, Number::wire))
                })
            }
            Some("i128" | "u128") => error(ty.span(), WIDE),
            _ => error(
                ty.span(),
                "a parameter is one of bool, a number, &str, &[u8], &mut [u8], data marked `#[hxn::data]` or an Option or Vec of data, &AtomicI32, &AtomicFlag, `&` or `&mut` of an interface, or `impl Fn(..) -> Result<T, E>`",
            ),
        }
    }

    // `Future<T>`, `Stream<T>` or `Signal<T>`, with the type it carries.
    fn later(&self, ty: &syn::Type) -> Result<Option<Type>, Error> {
        type Make = fn(Box<Type>) -> Type;
        let kinds: [(&str, Make); 3] = [
            ("Future", Type::Future),
            ("Stream", Type::Stream),
            ("Signal", Type::Signal),
        ];
        for (name, make) in kinds {
            let Some(carried) = wrapped(ty, name) else {
                continue;
            };
            return Ok(Some(make(Box::new(self.carried(name, carried)?))));
        }
        Ok(None)
    }

    // The carried type of a `Future`, `Stream` or `Signal`, whichever `name`
    // is.
    fn carried(&self, name: &str, carried: &syn::Type) -> Result<Type, Error> {
        let ty = carried;
        let plain = match path_name(carried).as_deref() {
            Some("bool") => Some(Type::Bool),
            Some("i32") => Some(Type::I32),
            Some("i64") => Some(Type::I64),
            Some("f64") => Some(Type::F64),
            Some("String") => Some(Type::Str),
            _ => None,
        };
        if let Some(plain) = plain {
            return Ok(plain);
        }
        // Bytes are transferred once, like an object.
        if name != "Signal" && wrapped(carried, "Vec").and_then(path_name).as_deref() == Some("u8")
        {
            return Ok(Type::Bytes);
        }
        // `()`, which still carries information: the work is done, or an
        // event happened. A signal always has a value.
        if name != "Signal" && matches!(carried, syn::Type::Tuple(tuple) if tuple.elems.is_empty())
        {
            return Ok(Type::Void);
        }
        // Data, and a numeric type with no Haxe equivalent.
        if let Some(data) = data_of(carried, self.known).filter(|data| data.crosses().is_some()) {
            return Ok(Type::Data(data));
        }
        if name != "Signal"
            && let Some(Type::Handle { interface, .. }) = self.listed(carried)?
        {
            return Ok(Type::List { interface });
        }
        // A signal is read any number of times, which a value with a single
        // owner cannot be. A future or a stream may carry no object.
        let (carried, nullable) = match optional(carried) {
            Some(inner) if name != "Signal" => (inner, true),
            _ => (carried, false),
        };
        let interface = if name == "Signal" {
            None
        } else if let Some(interface) = self.interface(carried) {
            if self.is_shared(&interface) {
                return error(
                    ty.span(),
                    format!(
                        "a shared interface is carried in its `Arc`: `{name}<Arc<{interface}>>`"
                    ),
                );
            }
            Some(interface)
        } else if let Some(interface) = self.pointer(carried) {
            if !self.is_shared(&interface) {
                return error(
                    ty.span(),
                    format!(
                        "`Arc<{interface}>` needs `{interface}` marked `#[hxn::interface(shared)]`"
                    ),
                );
            }
            Some(interface)
        } else {
            None
        };
        match interface {
            Some(interface) => Ok(Type::Handle {
                interface,
                nullable,
            }),
            None if name == "Signal" => error(
                ty.span(),
                "a signal carries one of bool, a number, String, data marked `#[hxn::data]` or an Option or Vec of data",
            ),
            None => error(
                ty.span(),
                format!(
                    "a {} carries one of (), bool, a number, String, Vec<u8>, data marked `#[hxn::data]` or an Option or Vec of data, or an interface or an Option or Vec of one",
                    name.to_lowercase()
                ),
            ),
        }
    }

    // The type, and whether it is a shared interface already in its `Arc`.
    fn result(&self, ty: &syn::Type) -> Result<(Type, bool), Error> {
        if let syn::Type::Tuple(tuple) = ty
            && tuple.elems.is_empty()
        {
            return Ok((Type::Void, false));
        }
        if let Some(later) = self.later(ty)? {
            return Ok((later, false));
        }
        let (inner, nullable) = match optional(ty) {
            Some(inner) => (inner, true),
            None => (ty, false),
        };
        if let Some(interface) = self.interface(inner) {
            return Ok((
                Type::Handle {
                    interface,
                    nullable,
                },
                false,
            ));
        }
        if let Some(interface) = self.pointer(inner) {
            if !self.is_shared(&interface) {
                return error(
                    ty.span(),
                    format!(
                        "`Arc<{interface}>` needs `{interface}` marked `#[hxn::interface(shared)]`"
                    ),
                );
            }
            return Ok((
                Type::Handle {
                    interface,
                    nullable,
                },
                true,
            ));
        }
        if self.borrowed(inner).is_some() {
            return error(
                ty.span(),
                "an interface is returned by value: a result cannot borrow",
            );
        }
        // A list of objects: a stream of them that has already ended.
        if let Some(element) = self.listed(ty)? {
            return Ok((Type::Stream(Box::new(element)), false));
        }
        if let Some(cell) = wrapped(ty, "Arc").and_then(cell) {
            return Ok((cell, false));
        }
        if cell(ty).is_some() {
            return error(
                ty.span(),
                "a cell the library keeps is returned as `Arc<AtomicI32>` or `Arc<AtomicFlag>`",
            );
        }
        if wrapped(ty, "Vec").and_then(path_name).as_deref() == Some("u8") {
            return Ok((Type::Bytes, false));
        }
        // `method` reads the data type from the signature.
        if let Some(wire) = self.shape(ty).and_then(|data| data.wire()) {
            return Ok((wire, false));
        }
        let plain = match path_name(ty).as_deref() {
            Some("bool") => Type::Bool,
            Some("i32") => Type::I32,
            Some("i64") => Type::I64,
            Some("f64") => Type::F64,
            Some("String") => Type::Str,
            // `method` reads the numeric type from the signature.
            Some(name) if Number::parse(name).is_some() => {
                Number::parse(name).map_or(Type::I32, Number::wire)
            }
            Some("i128" | "u128") => return error(ty.span(), WIDE),
            _ => {
                return error(
                    ty.span(),
                    "a result is one of (), bool, a number, String, Vec<u8>, data marked `#[hxn::data]` or an Option or Vec of data, Arc<AtomicI32>, Arc<AtomicFlag>, an interface or an Option of one, a Future, Stream or Signal, or a Result of these",
                );
            }
        };
        Ok((plain, false))
    }

    // Unwraps `Result<T, E>` to `T`, and reports that the function can fail.
    fn returns(&self, output: &ReturnType) -> Result<((Type, bool), bool), Error> {
        match given(output) {
            None => Ok(((Type::Void, false), false)),
            Some((ty, fallible)) => Ok((self.result(ty)?, fallible)),
        }
    }

    // An `async fn` as the host sees it: a function that returns a future of
    // its output. It runs after the call that starts it has returned, so it
    // must own all its parameters by then.
    fn started(
        &self,
        signature: &syn::Signature,
        receiver: Receiver,
        params: &[Param],
        constructor: bool,
    ) -> Result<((Type, bool), bool), Error> {
        const LATER: &str = "an async function runs after the call that starts it has returned";
        let name = &signature.ident;
        if constructor {
            return error(
                name.span(),
                "`new` is not async: a Haxe `new` gives its object at once",
            );
        }
        let borrows = match receiver {
            Receiver::None | Receiver::Owned => false,
            Receiver::Shared => !self.is_shared(self.own),
            Receiver::Exclusive => true,
        };
        if borrows {
            return error(
                name.span(),
                format!(
                    "{LATER}, so it cannot borrow the object: take `self`, or mark the interface `shared`"
                ),
            );
        }
        for param in params {
            let lent = match &param.ty {
                Type::Lent { .. } => Some("a function"),
                Type::AtomicInt | Type::AtomicBool => Some("a cell"),
                Type::BytesMut => Some("bytes to write into"),
                Type::Handle { interface, .. }
                    if !param.taken && (param.exclusive || !self.is_shared(interface)) =>
                {
                    Some("an object")
                }
                _ => None,
            };
            if let Some(lent) = lent {
                return error(
                    name.span(),
                    format!(
                        "{LATER}, so it cannot take {lent}, which is lent for the call: `{}`",
                        param.name
                    ),
                );
            }
        }
        let (carried, fallible) = match given(&signature.output) {
            None => (Type::Void, false),
            Some((ty, fallible)) => {
                if self.later(ty)?.is_some() {
                    return error(
                        ty.span(),
                        "an async function gives its value when it is done: return the value, not a Future of it",
                    );
                }
                if let Some(interface) = self.interface(ty)
                    && self.is_shared(&interface)
                {
                    return error(
                        ty.span(),
                        format!("a shared interface is given in its `Arc`: `Arc<{interface}>`"),
                    );
                }
                (self.carried("Future", ty)?, fallible)
            }
        };
        Ok(((Type::Future(Box::new(carried)), false), fallible))
    }

    fn method(
        &self,
        signature: &syn::Signature,
        attributes: &[Attribute],
    ) -> Result<Method, Error> {
        let name = signature.ident.to_string();
        // A callback may fail with an error type the caller chooses, which is
        // the only allowed use of a type parameter here.
        let lends = signature.inputs.iter().any(|input| {
            matches!(input, FnArg::Typed(typed) if matches!(&*typed.ty, syn::Type::ImplTrait(_)))
        });
        let only_types = signature
            .generics
            .params
            .iter()
            .all(|param| matches!(param, syn::GenericParam::Type(_)));
        if signature.generics.where_clause.is_some()
            || !only_types
            || (!signature.generics.params.is_empty() && !lends)
        {
            return error(signature.generics.span(), "a generic function cannot cross");
        }
        let asynchronous = signature.asyncness.is_some();
        if let Some(token) = &signature.unsafety {
            return error(token.span(), "an unsafe function cannot cross");
        }

        let mut receiver = Receiver::None;
        let mut params = Vec::new();
        for input in &signature.inputs {
            match input {
                FnArg::Receiver(this) => {
                    if this.colon_token.is_some() {
                        return error(this.span(), "a receiver is `&self`, `&mut self` or `self`");
                    }
                    receiver = match (&this.reference, &this.mutability) {
                        (Some(_), None) => Receiver::Shared,
                        (Some(_), Some(_)) => Receiver::Exclusive,
                        (None, _) => Receiver::Owned,
                    };
                }
                FnArg::Typed(typed) => {
                    let syn::Pat::Ident(pattern) = &*typed.pat else {
                        return error(typed.pat.span(), "a parameter needs a plain name");
                    };
                    let name = pattern.ident.to_string();
                    // Names the adapters use for their own variables.
                    if ["this", "out", "err"].contains(&name.as_str()) {
                        return error(
                            pattern.ident.span(),
                            format!(
                                "`{name}` is a name the adapters use; call the parameter something else"
                            ),
                        );
                    }
                    params.push(self.param(name, &typed.ty)?);
                }
            }
        }

        let shared = self.is_shared(self.own);
        if shared && matches!(receiver, Receiver::Exclusive | Receiver::Owned) {
            return error(
                signature.ident.span(),
                "a shared interface is reached from more than one place: its functions take `&self`, and change it through a cell or a lock inside",
            );
        }
        if shared && ["same", "weak"].contains(&name.as_str()) {
            return error(
                signature.ident.span(),
                format!("every shared interface has a `{name}` of hxn's"),
            );
        }
        let constructor = name == "new";
        let ((returns, returns_arc), fallible) = if asynchronous {
            self.started(signature, receiver, &params, constructor)?
        } else {
            self.returns(&signature.output)?
        };
        let own = Type::Handle {
            interface: self.own.to_owned(),
            nullable: false,
        };
        if constructor && (receiver != Receiver::None || returns != own) {
            return error(
                signature.ident.span(),
                "`new` is the constructor: no `self`, and it returns the interface",
            );
        }
        let blocking = is_blocking(attributes)?;
        if blocking && asynchronous {
            return error(
                signature.ident.span(),
                "an async function does not wait: the future it gives is what is waited on",
            );
        }
        if blocking
            && params
                .iter()
                .any(|param| matches!(param.ty, Type::Lent { .. }))
        {
            return error(
                signature.ident.span(),
                "a function that waits cannot be lent a Haxe function: the collector may run while it waits",
            );
        }
        Ok(Method {
            name,
            docs: docs(attributes),
            receiver,
            params,
            returns,
            fallible,
            asynchronous,
            blocking,
            constructor,
            returns_arc,
            returns_list: !asynchronous
                && given(&signature.output)
                    .is_some_and(|(ty, _)| matches!(self.listed(ty), Ok(Some(_)))),
            returns_number: given(&signature.output)
                .filter(|_| !asynchronous)
                .and_then(|(ty, _)| path_name(ty))
                .and_then(|name| Number::parse(&name)),
            returns_data: given(&signature.output)
                .filter(|_| !asynchronous)
                .and_then(|(ty, _)| self.shape(ty)),
            error: failing(&signature.output)
                .and_then(|ty| data_of(ty, self.known))
                .filter(|data| matches!(data, Data::Enum(_) | Data::Record(_) | Data::Tagged(_))),
            free: false,
        })
    }
}

const WIDE: &str = "an integer wider than 64 bits does not cross: Haxe has none. Split the value";

// A function's return type without its `Result`, and whether it had one.
fn given(output: &ReturnType) -> Option<(&syn::Type, bool)> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    if let syn::Type::Path(path) = &**ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Result"
        && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
        && let Some(syn::GenericArgument::Type(ok)) = arguments.args.first()
    {
        return Some((ok, true));
    }
    Some((ty, false))
}

// The error of a function's `Result`, where it names one.
fn failing(output: &ReturnType) -> Option<&syn::Type> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    let syn::Type::Path(path) = &**ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.iter().nth(1) {
        Some(syn::GenericArgument::Type(error)) => Some(error),
        _ => None,
    }
}

/// One file's items, including those of its inline modules, and its path if
/// it was read from a file.
struct Source {
    file: Option<String>,
    items: Vec<Item>,
}

// A module declared in one file and defined in another.
struct Outside {
    name: String,
    line: usize,
    // `#[path = "..."]`, which is relative to the declaring file.
    path: Option<String>,
    // The directory its file is searched for in otherwise.
    dir: PathBuf,
}

fn path_attribute(attributes: &[Attribute]) -> Option<String> {
    attributes
        .iter()
        .find_map(|attribute| match &attribute.meta {
            Meta::NameValue(pair) if pair.path.is_ident("path") => match &pair.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(text),
                    ..
                }) => Some(text.value()),
                _ => None,
            },
            _ => None,
        })
}

// Flattens a file's items: items of inline modules are added to the rest,
// and a module defined in another file is recorded. `dir` is the directory
// of this level's module files.
fn flatten(items: Vec<Item>, dir: &Path, out: &mut Vec<Item>, outside: &mut Vec<Outside>) {
    for item in items {
        match item {
            Item::Mod(module) => {
                let name = module.ident.to_string();
                match module.content {
                    Some((_, inner)) => flatten(inner, &dir.join(&name), out, outside),
                    None => outside.push(Outside {
                        line: module.ident.span().start().line,
                        path: path_attribute(&module.attrs),
                        dir: dir.to_owned(),
                        name,
                    }),
                }
            }
            other => out.push(other),
        }
    }
}

fn parse_text(text: &str, file: Option<&Path>) -> Result<syn::File, Error> {
    syn::parse_file(text).map_err(|error| Error {
        file: file.map(|file| file.display().to_string()),
        line: error.span().start().line,
        message: error.to_string(),
    })
}

// Reads the file at `path`, whose submodules' files are in `dir`, and then
// those submodules.
fn load(
    path: &Path,
    dir: &Path,
    sources: &mut Vec<Source>,
    files: &mut Vec<PathBuf>,
) -> Result<(), Error> {
    let name = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|error| Error {
        file: Some(name.clone()),
        line: 0,
        message: error.to_string(),
    })?;
    let file = parse_text(&text, Some(path))?;
    files.push(path.to_owned());
    let mut items = Vec::new();
    let mut outside = Vec::new();
    flatten(file.items, dir, &mut items, &mut outside);
    sources.push(Source {
        file: Some(name.clone()),
        items,
    });
    let beside = path.parent().unwrap_or(Path::new(""));
    for module in outside {
        // A file named after its module keeps its submodules in a directory
        // of the same name; `mod.rs`, and a file given by `#[path]`, keep
        // them in their own directory.
        let beside_it = module.dir.join(format!("{}.rs", module.name));
        let within = module.dir.join(&module.name).join("mod.rs");
        let (file, dir) = if let Some(given) = &module.path {
            let file = beside.join(given);
            let dir = file.parent().unwrap_or(Path::new("")).to_owned();
            (file, dir)
        } else if beside_it.is_file() {
            (beside_it, module.dir.join(&module.name))
        } else if within.is_file() {
            (within, module.dir.join(&module.name))
        } else {
            return Err(Error {
                file: Some(name),
                line: module.line,
                message: format!(
                    "the file of module `{}` was not found: neither {} nor {}",
                    module.name,
                    beside_it.display(),
                    within.display()
                ),
            });
        };
        load(&file, &dir, sources, files)?;
    }
    Ok(())
}

// Adds the file an error occurred in, if the error has no file yet.
fn located<T>(result: Result<T, Error>, source: &Source) -> Result<T, Error> {
    result.map_err(|mut error| {
        if error.file.is_none() {
            error.file.clone_from(&source.file);
        }
        error
    })
}

// Names the adapters use for their own exports.
const RESERVED: [&str; 6] = ["Atomic", "Memory", "Future", "Stream", "Signal", "Outbox"];

// The names of the library's data types, used to resolve a signature's
// types.
#[derive(Clone, Copy)]
struct Known<'a> {
    records: &'a [String],
    // The C-like enums, and the tagged enums.
    enums: &'a [String],
    tagged: &'a [String],
}

// The names of a library's data types, as they are collected.
#[derive(Default)]
struct Names {
    records: Vec<String>,
    enums: Vec<String>,
    tagged: Vec<String>,
}

// A `#[hxn::data]` item's name, checked the same way as an interface's.
fn data_name(
    item: &syn::Ident,
    vis: &Visibility,
    generics: &syn::Generics,
    taken: &[String],
) -> Result<String, Error> {
    if !matches!(vis, Visibility::Public(_)) {
        return error(item.span(), "data has to be `pub`");
    }
    if !generics.params.is_empty() {
        return error(generics.span(), "generic data cannot cross");
    }
    let name = item.to_string();
    if RESERVED.contains(&name.as_str()) {
        return error(
            item.span(),
            format!("`{name}` is a name the adapters use; call the data something else"),
        );
    }
    if taken.contains(&name) {
        return error(item.span(), format!("`{name}` is marked twice"));
    }
    Ok(name)
}

// Collects the names of every `#[hxn::data]` struct and enum before any
// field is read: a field's type may be another data type, declared later or
// in another file.
fn data_names(items: &[Item], taken: &mut Vec<String>, names: &mut Names) -> Result<(), Error> {
    for item in items {
        match item {
            Item::Struct(item) if item.attrs.iter().any(|a| is_marker(a, "data")) => {
                let name = data_name(&item.ident, &item.vis, &item.generics, taken)?;
                taken.push(name.clone());
                names.records.push(name);
            }
            Item::Enum(item) if item.attrs.iter().any(|a| is_marker(a, "data")) => {
                let name = data_name(&item.ident, &item.vis, &item.generics, taken)?;
                taken.push(name.clone());
                if holds_values(item) {
                    names.tagged.push(name);
                } else {
                    names.enums.push(name);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn holds_values(item: &syn::ItemEnum) -> bool {
    item.variants
        .iter()
        .any(|variant| !matches!(variant.fields, syn::Fields::Unit))
}

// The data type of a field.
fn field_data(ty: &syn::Type, known: Known) -> Result<Data, Error> {
    let Some(data) = data_of(ty, known) else {
        return error(
            ty.span(),
            "a field of data is one of bool, a number, String, Vec<u8>, other data, a nonempty tuple, or an Option, Vec, Box, HashMap, BTreeMap, HashSet or BTreeSet of these, a map keyed by String, a whole number of at most 32 bits or a C-like enum",
        );
    };
    if matches!(data, Data::Number(Number::Isize | Number::Usize)) {
        return error(
            ty.span(),
            "a size is as wide as the machine: give a field of data a width, such as `u32`",
        );
    }
    Ok(data)
}

// The variants of a tagged enum.
fn cases(item: &syn::ItemEnum, known: Known) -> Result<Vec<Case>, Error> {
    let mut cases = Vec::new();
    for variant in &item.variants {
        if let Some((_, number)) = &variant.discriminant {
            return error(
                number.span(),
                "a variant is given a number only in an enum whose variants all hold nothing",
            );
        }
        let named = matches!(variant.fields, syn::Fields::Named(_));
        let count = variant.fields.len();
        let mut fields = Vec::new();
        for (index, field) in variant.fields.iter().enumerate() {
            let name = match &field.ident {
                Some(name) => name.to_string(),
                None if count == 1 => String::from("value"),
                None => format!("value{index}"),
            };
            fields.push(Field {
                name,
                docs: docs(&field.attrs),
                ty: field_data(&field.ty, known)?,
            });
        }
        cases.push(Case {
            name: variant.ident.to_string(),
            docs: docs(&variant.attrs),
            fields,
            named,
        });
    }
    Ok(cases)
}

// A variant's integer value: the explicit discriminant, or one more than the
// previous variant's.
fn discriminant(variant: &syn::Variant, next: i32) -> Result<i32, Error> {
    const NUMBER: &str =
        "a variant's number is written as a whole number that fits an i32, or left out";
    let Some((_, value)) = &variant.discriminant else {
        return Ok(next);
    };
    let (literal, negative) = match value {
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => (&*unary.expr, true),
        other => (other, false),
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Int(number),
        ..
    }) = literal
    else {
        return error(value.span(), NUMBER);
    };
    let Ok(number) = number.base10_parse::<i64>() else {
        return error(value.span(), NUMBER);
    };
    i32::try_from(if negative { -number } else { number }).or_else(|_| error(value.span(), NUMBER))
}

// Parses the records and enums once every name is known.
fn data_items(
    items: &[Item],
    known: Known,
    records: &mut Vec<Record>,
    enums: &mut Vec<Enumeration>,
    tagged: &mut Vec<Tagged>,
) -> Result<(), Error> {
    for item in items {
        match item {
            Item::Struct(item) if item.attrs.iter().any(|a| is_marker(a, "data")) => {
                let syn::Fields::Named(named) = &item.fields else {
                    return error(
                        item.ident.span(),
                        "data is a struct of named fields, or an enum",
                    );
                };
                let mut fields = Vec::new();
                for field in &named.named {
                    let name = field
                        .ident
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    // The adapter builds the value field by field.
                    if !matches!(field.vis, Visibility::Public(_)) {
                        return error(
                            field.span(),
                            format!(
                                "every field of data is `pub`: the adapter builds it, and `{name}` is not"
                            ),
                        );
                    }
                    let ty = field_data(&field.ty, known)?;
                    fields.push(Field {
                        name,
                        docs: docs(&field.attrs),
                        ty,
                    });
                }
                records.push(Record {
                    name: item.ident.to_string(),
                    docs: docs(&item.attrs),
                    fields,
                });
            }
            Item::Enum(item) if item.attrs.iter().any(|a| is_marker(a, "data")) => {
                if item.variants.is_empty() {
                    return error(
                        item.ident.span(),
                        "an enum with no variants has no value to cross",
                    );
                }
                if holds_values(item) {
                    tagged.push(Tagged {
                        name: item.ident.to_string(),
                        docs: docs(&item.attrs),
                        cases: cases(item, known)?,
                    });
                    continue;
                }
                let mut variants: Vec<Variant> = Vec::new();
                let mut next = 0;
                for variant in &item.variants {
                    let value = discriminant(variant, next)?;
                    if variants.iter().any(|known| known.value == value) {
                        return error(variant.span(), format!("two variants are {value}"));
                    }
                    next = value.wrapping_add(1);
                    variants.push(Variant {
                        name: variant.ident.to_string(),
                        docs: docs(&variant.attrs),
                        value,
                    });
                }
                enums.push(Enumeration {
                    name: item.ident.to_string(),
                    docs: docs(&item.attrs),
                    variants,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

// The structs a file marks `#[hxn::interface]`.
fn interfaces_of(items: &[Item], interfaces: &mut Vec<Interface>) -> Result<(), Error> {
    for item in items {
        let Item::Struct(item) = item else { continue };
        if !item.attrs.iter().any(|a| is_marker(a, "interface")) {
            continue;
        }
        if !matches!(item.vis, Visibility::Public(_)) {
            return error(item.ident.span(), "an interface has to be `pub`");
        }
        if !item.generics.params.is_empty() {
            return error(item.generics.span(), "a generic struct cannot cross");
        }
        let name = item.ident.to_string();
        if RESERVED.contains(&name.as_str()) {
            return error(
                item.ident.span(),
                format!("`{name}` is a name the adapters use; call the interface something else"),
            );
        }
        // Names are unique across the library, whatever module an item is in.
        if interfaces.iter().any(|known| known.name == name) {
            return error(
                item.ident.span(),
                format!("`{name}` is marked as an interface twice"),
            );
        }
        let notes = notes(&item.attrs)?;
        interfaces.push(Interface {
            name,
            docs: docs(&item.attrs),
            shared: notes.shared,
            weighed: notes.weighed,
            namespace: false,
            methods: Vec::new(),
        });
    }
    Ok(())
}

// A shared interface's weak reference is a class named after it with a
// `Weak` suffix.
fn weak_clash(items: &[Item], names: &[String], shared: &[String]) -> Result<(), Error> {
    for name in shared {
        let weak = format!("{name}Weak");
        for item in items {
            if let Item::Struct(item) = item
                && item.ident == weak
                && names.contains(&weak)
            {
                return error(
                    item.ident.span(),
                    format!(
                        "`{weak}` is the weak reference to `{name}`; call the interface something else"
                    ),
                );
            }
        }
    }
    Ok(())
}

// The functions a file's marked blocks expose, each added to its interface.
fn methods_of(
    items: &[Item],
    interfaces: &mut [Interface],
    names: &[String],
    shared: &[String],
    known: Known,
) -> Result<(), Error> {
    for item in items {
        let Item::Impl(block) = item else { continue };
        if !block.attrs.iter().any(|a| is_marker(a, "methods")) {
            continue;
        }
        if block.trait_.is_some() {
            return error(block.span(), "`#[hxn::methods]` goes on an inherent impl");
        }
        let Some(target) = path_name(&block.self_ty) else {
            return error(block.self_ty.span(), "expected the name of an interface");
        };
        let Some(interface) = interfaces.iter_mut().find(|i| i.name == target) else {
            return error(
                block.self_ty.span(),
                format!("`{target}` is not marked `#[hxn::interface]`"),
            );
        };
        let scope = Scope {
            interfaces: names,
            shared,
            own: &target,
            known,
        };
        for item in &block.items {
            let ImplItem::Fn(function) = item else {
                continue;
            };
            if !matches!(function.vis, Visibility::Public(_)) {
                continue;
            }
            let method = scope.method(&function.sig, &function.attrs)?;
            if interface.methods.iter().any(|m| m.name == method.name) {
                return error(
                    function.sig.ident.span(),
                    format!("`{}` is exposed twice", method.name),
                );
            }
            if method.name == "close" {
                return error(
                    function.sig.ident.span(),
                    "`close` is what releases the value, and every interface has one",
                );
            }
            interface.methods.push(method);
        }
    }
    Ok(())
}

// What `#[hxn::function]` specifies for a function, if it is marked: the
// class it is a static function of, when that is not the library's own.
fn function_class(attributes: &[Attribute]) -> Result<Option<Option<String>>, Error> {
    let Some(attribute) = attributes.iter().find(|a| is_marker(a, "function")) else {
        return Ok(None);
    };
    if matches!(attribute.meta, Meta::Path(_)) {
        return Ok(Some(None));
    }
    let mut class = None;
    let parsed = attribute.parse_nested_meta(|note| {
        if !note.path.is_ident("class") {
            return Err(note.error("unknown note"));
        }
        let name: syn::LitStr = note.value()?.parse()?;
        class = Some(name.value());
        Ok(())
    });
    let named = class.as_deref().is_some_and(|name| {
        let mut letters = name.chars();
        letters
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
            && letters.all(|letter| letter.is_ascii_alphanumeric())
    });
    if parsed.is_err() || !named {
        return error(
            attribute.span(),
            "expected `#[hxn::function]` or `#[hxn::function(class = \"Name\")]`, the name a Haxe class can have",
        );
    }
    Ok(Some(class))
}

// The functions a file marks outside any `impl`, each added as a static
// function of its class: an interface's, or one that exists only to hold
// them.
fn functions_of(
    items: &[Item],
    library: &str,
    interfaces: &mut Vec<Interface>,
    names: &[String],
    shared: &[String],
    known: Known,
) -> Result<(), Error> {
    for item in items {
        let Item::Fn(function) = item else { continue };
        let Some(class) = function_class(&function.attrs)? else {
            continue;
        };
        let name = &function.sig.ident;
        if !matches!(function.vis, Visibility::Public(_)) {
            return error(name.span(), "a function exposed to Haxe has to be `pub`");
        }
        let class = class.unwrap_or_else(|| pascal(library));
        if RESERVED.contains(&class.as_str()) {
            return error(
                name.span(),
                format!("`{class}` is a name the adapters use; put the function on another class"),
            );
        }
        if name == "new" {
            return error(
                name.span(),
                "`new` makes an interface's value: a function outside an `impl` has another name",
            );
        }
        let scope = Scope {
            interfaces: names,
            shared,
            own: &class,
            known,
        };
        let mut method = scope.method(&function.sig, &function.attrs)?;
        method.free = true;
        let at = match interfaces.iter().position(|known| known.name == class) {
            Some(at) => at,
            None => {
                interfaces.push(Interface {
                    name: class.clone(),
                    docs: Vec::new(),
                    shared: false,
                    weighed: false,
                    namespace: true,
                    methods: Vec::new(),
                });
                interfaces.len() - 1
            }
        };
        let interface = &mut interfaces[at];
        if interface.methods.iter().any(|m| m.name == method.name) {
            return error(
                name.span(),
                format!("`{class}` already has a function named `{}`", method.name),
            );
        }
        if method.name == "close" && !interface.namespace {
            return error(
                name.span(),
                "`close` is what releases the value, and every interface has one",
            );
        }
        interface.methods.push(method);
    }
    Ok(())
}

fn build(name: &str, sources: &[Source]) -> Result<Library, Error> {
    // Every interface is collected first, so that a `#[hxn::methods]` block
    // in one file can name an interface declared in another.
    let mut interfaces: Vec<Interface> = Vec::new();
    for source in sources {
        located(interfaces_of(&source.items, &mut interfaces), source)?;
    }
    let names: Vec<String> = interfaces.iter().map(|i| i.name.clone()).collect();
    let shared: Vec<String> = interfaces
        .iter()
        .filter(|i| i.shared)
        .map(|i| i.name.clone())
        .collect();
    for source in sources {
        located(weak_clash(&source.items, &names, &shared), source)?;
    }
    // Data shares its names with the interfaces and their weak classes.
    let mut taken = names.clone();
    taken.extend(shared.iter().map(|name| crate::names::weak_class(name)));
    let mut data = Names::default();
    for source in sources {
        located(data_names(&source.items, &mut taken, &mut data), source)?;
    }
    // A tagged enum is serialized and deserialized by a class named after it.
    for name in &data.tagged {
        let wire = wire_class(name);
        if taken.contains(&wire) {
            return Err(Error {
                file: None,
                line: 0,
                message: format!(
                    "`{wire}` is the class that writes and reads `{name}`; call the other something else"
                ),
            });
        }
    }
    let known = Known {
        records: &data.records,
        enums: &data.enums,
        tagged: &data.tagged,
    };
    let (mut records, mut enums, mut tagged) = (Vec::new(), Vec::new(), Vec::new());
    for source in sources {
        located(
            data_items(&source.items, known, &mut records, &mut enums, &mut tagged),
            source,
        )?;
    }
    for source in sources {
        located(
            methods_of(&source.items, &mut interfaces, &names, &shared, known),
            source,
        )?;
    }
    // After every `impl`, so that a function's class already holds its
    // methods.
    for source in sources {
        located(
            functions_of(&source.items, name, &mut interfaces, &names, &shared, known),
            source,
        )?;
    }
    let library = Library {
        name: name.to_owned(),
        interfaces,
        records,
        enums,
        tagged,
    };
    // An error's exception is a class named after it.
    for error in library.errors() {
        let exception = exception_class(&error.name());
        if taken.contains(&exception) {
            return Err(Error {
                file: None,
                line: 0,
                message: format!(
                    "`{exception}` is the exception a failure with `{}` is thrown as; call the other something else",
                    error.name()
                ),
            });
        }
    }
    if let Some(message) = crate::later::clash(&library) {
        return Err(Error {
            file: None,
            line: 0,
            message,
        });
    }
    Ok(library)
}

/// Parses the marked items of `source`, the text of one Rust file, as the
/// API of the library `name`. Modules defined in other files are not read:
/// [`parse_crate`] reads a library from its files.
pub fn parse(name: &str, source: &str) -> Result<Library, Error> {
    let file = parse_text(source, None)?;
    let mut items = Vec::new();
    flatten(file.items, Path::new(""), &mut items, &mut Vec::new());
    build(name, &[Source { file: None, items }])
}

/// Reads the library `name` from the file at `root`, its `lib.rs`, and from
/// the file of every module declared from there down. Also returns the files
/// read, for a build script to watch.
///
/// The adapters refer to an interface by its path at the crate's root, so one
/// defined in a module must be re-exported there.
pub fn parse_crate(name: &str, root: &Path) -> Result<(Library, Vec<PathBuf>), Error> {
    let mut sources = Vec::new();
    let mut files = Vec::new();
    let dir = root.parent().unwrap_or(Path::new(""));
    load(root, dir, &mut sources, &mut files)?;
    Ok((build(name, &sources)?, files))
}
