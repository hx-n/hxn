//! From a library's source to its [`Library`]: the items it marked, and
//! nothing the emitters cannot carry.

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Attribute, FnArg, ImplItem, Item, Meta, ReturnType, Visibility};

use crate::model::{Error, Interface, Library, Method, Param, Receiver, Type};

fn error<T>(span: Span, message: impl Into<String>) -> Result<T, Error> {
    Err(Error {
        line: span.start().line,
        message: message.into(),
    })
}

// `#[hxn::interface]` and `#[hxn::methods]`.
fn is_marker(attribute: &Attribute, name: &str) -> bool {
    let segments: Vec<_> = attribute
        .path()
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    segments == ["hxn", name]
}

/// What `#[hxn::interface(...)]` says of a struct.
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

// The `#[hxn(...)]` notes on a function. `blocking` is the only one.
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

// `Option<T>` gives `T`.
fn optional(ty: &syn::Type) -> Option<&syn::Type> {
    wrapped(ty, "Option")
}

// The cell `ty` names: `AtomicI32` or `AtomicFlag`.
fn cell(ty: &syn::Type) -> Option<Type> {
    match path_name(ty)?.as_str() {
        "AtomicI32" => Some(Type::AtomicInt),
        "AtomicFlag" => Some(Type::AtomicBool),
        _ => None,
    }
}

// `wrapper<T>` gives `T`.
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

    // `Arc<T>` of an interface gives `T`.
    fn pointer(&self, ty: &syn::Type) -> Option<String> {
        self.interface(wrapped(ty, "Arc")?)
    }

    fn is_shared(&self, interface: &str) -> bool {
        self.shared.iter().any(|shared| shared == interface)
    }

    fn param(&self, name: String, ty: &syn::Type) -> Result<Param, Error> {
        let plain = |ty| Param {
            name: name.clone(),
            ty,
            exclusive: false,
            kept: false,
        };
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
        if self.interface(ty).is_some() {
            return error(
                ty.span(),
                "an interface is taken as `&T` or `&mut T`, not by value",
            );
        }
        match path_name(ty).as_deref() {
            Some("bool") => Ok(plain(Type::Bool)),
            Some("i32") => Ok(plain(Type::I32)),
            Some("f64") => Ok(plain(Type::F64)),
            _ => error(
                ty.span(),
                "a parameter is one of bool, i32, f64, &str, &[u8], &AtomicI32, &AtomicFlag, or `&` or `&mut` of an interface",
            ),
        }
    }

    // `Future<T>`, `Stream<T>` or `Signal<T>`, with what it carries.
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
            let plain = match path_name(carried).as_deref() {
                Some("bool") => Some(Type::Bool),
                Some("i32") => Some(Type::I32),
                Some("f64") => Some(Type::F64),
                Some("String") => Some(Type::Str),
                _ => None,
            };
            if let Some(plain) = plain {
                return Ok(Some(make(Box::new(plain))));
            }
            // A signal is read any number of times, which a value held
            // by one owner cannot be.
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
            return match interface {
                Some(interface) => Ok(Some(make(Box::new(Type::Handle {
                    interface,
                    nullable: false,
                })))),
                None if name == "Signal" => error(
                    ty.span(),
                    "a signal carries one of bool, i32, f64 or String",
                ),
                None => error(
                    ty.span(),
                    format!(
                        "a {} carries one of bool, i32, f64, String or an interface",
                        name.to_lowercase()
                    ),
                ),
            };
        }
        Ok(None)
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
        let plain = match path_name(ty).as_deref() {
            Some("bool") => Type::Bool,
            Some("i32") => Type::I32,
            Some("f64") => Type::F64,
            Some("String") => Type::Str,
            _ => {
                return error(
                    ty.span(),
                    "a result is one of (), bool, i32, f64, String, Vec<u8>, Arc<AtomicI32>, Arc<AtomicFlag>, an interface or an Option of one, a Future, Stream or Signal, or a Result of these",
                );
            }
        };
        Ok((plain, false))
    }

    // `Result<T, E>` gives `T` and says the function can fail.
    fn returns(&self, output: &ReturnType) -> Result<((Type, bool), bool), Error> {
        let ReturnType::Type(_, ty) = output else {
            return Ok(((Type::Void, false), false));
        };
        if let syn::Type::Path(path) = &**ty
            && let Some(segment) = path.path.segments.last()
            && segment.ident == "Result"
            && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
            && let Some(syn::GenericArgument::Type(ok)) = arguments.args.first()
        {
            return Ok((self.result(ok)?, true));
        }
        Ok((self.result(ty)?, false))
    }

    fn method(&self, function: &syn::ImplItemFn) -> Result<Method, Error> {
        let signature = &function.sig;
        let name = signature.ident.to_string();
        if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
            return error(signature.generics.span(), "a generic function cannot cross");
        }
        if let Some(token) = &signature.asyncness {
            return error(token.span(), "an async function cannot cross yet");
        }
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
                    // What the adapters call their own.
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
        let ((returns, returns_arc), fallible) = self.returns(&signature.output)?;
        let own = Type::Handle {
            interface: self.own.to_owned(),
            nullable: false,
        };
        let constructor = name == "new";
        if constructor && (receiver != Receiver::None || returns != own) {
            return error(
                signature.ident.span(),
                "`new` is the constructor: no `self`, and it returns the interface",
            );
        }
        Ok(Method {
            name,
            docs: docs(&function.attrs),
            receiver,
            params,
            returns,
            fallible,
            blocking: is_blocking(&function.attrs)?,
            constructor,
            returns_arc,
        })
    }
}

/// Reads the marked items of `source`, the text of one Rust file, as the
/// API of the library `name`.
pub fn parse(name: &str, source: &str) -> Result<Library, Error> {
    let file = syn::parse_file(source).map_err(|error| Error {
        line: error.span().start().line,
        message: error.to_string(),
    })?;

    let mut interfaces: Vec<Interface> = Vec::new();
    for item in &file.items {
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
        // What the adapters name their own exports after.
        if ["Atomic", "Memory", "Future", "Stream", "Signal", "Outbox"].contains(&name.as_str()) {
            return error(
                item.ident.span(),
                format!("`{name}` is a name the adapters use; call the interface something else"),
            );
        }
        let notes = notes(&item.attrs)?;
        interfaces.push(Interface {
            name,
            docs: docs(&item.attrs),
            shared: notes.shared,
            weighed: notes.weighed,
            methods: Vec::new(),
        });
    }
    let names: Vec<String> = interfaces.iter().map(|i| i.name.clone()).collect();
    let shared: Vec<String> = interfaces
        .iter()
        .filter(|i| i.shared)
        .map(|i| i.name.clone())
        .collect();
    // A shared interface's weak reference is a class of that name.
    for name in &shared {
        let weak = format!("{name}Weak");
        for item in &file.items {
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

    for item in &file.items {
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
            interfaces: &names,
            shared: &shared,
            own: &target,
        };
        for item in &block.items {
            let ImplItem::Fn(function) = item else {
                continue;
            };
            if !matches!(function.vis, Visibility::Public(_)) {
                continue;
            }
            let method = scope.method(function)?;
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

    Ok(Library {
        name: name.to_owned(),
        interfaces,
    })
}
