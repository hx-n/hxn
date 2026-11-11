//! From a library's source to its [`Library`]: the items it marked, and
//! nothing the emitters cannot carry.

use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Attribute, FnArg, ImplItem, Item, Meta, ReturnType, Visibility};

use crate::model::{Error, Interface, Library, Method, Number, Param, Receiver, Type};

fn error<T>(span: Span, message: impl Into<String>) -> Result<T, Error> {
    Err(Error {
        file: None,
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

// `wrapper<T, ..>` gives `T`.
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

    // `impl Fn(i32, f64) -> Result<bool, E>`, or `FnMut` or `FnOnce`.
    fn lent(&self, ty: &syn::TypeImplTrait) -> Result<Type, Error> {
        const SHAPE: &str = "a function is lent as `impl Fn(..) -> Result<T, E>`, taking bool, i32 or f64 and giving one of those or `()`: it can fail, because the Haxe function can throw";
        let scalar = |ty: &syn::Type| match path_name(ty).as_deref() {
            Some("bool") => Some(Type::Bool),
            Some("i32") => Some(Type::I32),
            Some("f64") => Some(Type::F64),
            _ => None,
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
            match scalar(input) {
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
            other => match scalar(other) {
                Some(given) => given,
                None => return error(other.span(), SHAPE),
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
            number: None,
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
                number: None,
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
                number: None,
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
                "a parameter is one of bool, a number, &str, &[u8], &AtomicI32, &AtomicFlag, `&` or `&mut` of an interface, or `impl Fn(..) -> Result<T, E>`",
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
            Some("i64") => Type::I64,
            Some("f64") => Type::F64,
            Some("String") => Type::Str,
            // Which number it is, `method` reads off the signature.
            Some(name) if Number::parse(name).is_some() => {
                Number::parse(name).map_or(Type::I32, Number::wire)
            }
            Some("i128" | "u128") => return error(ty.span(), WIDE),
            _ => {
                return error(
                    ty.span(),
                    "a result is one of (), bool, a number, String, Vec<u8>, Arc<AtomicI32>, Arc<AtomicFlag>, an interface or an Option of one, a Future, Stream or Signal, or a Result of these",
                );
            }
        };
        Ok((plain, false))
    }

    // `Result<T, E>` gives `T` and says the function can fail.
    fn returns(&self, output: &ReturnType) -> Result<((Type, bool), bool), Error> {
        match given(output) {
            None => Ok(((Type::Void, false), false)),
            Some((ty, fallible)) => Ok((self.result(ty)?, fallible)),
        }
    }

    fn method(&self, function: &syn::ImplItemFn) -> Result<Method, Error> {
        let signature = &function.sig;
        let name = signature.ident.to_string();
        // A function lent to the call may fail with an error of the caller's
        // choosing, which is the one use a type parameter has here.
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
        let blocking = is_blocking(&function.attrs)?;
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
            docs: docs(&function.attrs),
            receiver,
            params,
            returns,
            fallible,
            blocking,
            constructor,
            returns_arc,
            returns_number: given(&signature.output)
                .and_then(|(ty, _)| path_name(ty))
                .and_then(|name| Number::parse(&name)),
        })
    }
}

const WIDE: &str = "an integer wider than 64 bits does not cross: Haxe has none. Split the value";

// What a function gives, less the `Result` around it, and whether it had one.
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

/// One file's items, those of its inline modules among them, and where it
/// is if it came from a file.
struct Source {
    file: Option<String>,
    items: Vec<Item>,
}

// A module declared in a file and kept in another.
struct Outside {
    name: String,
    line: usize,
    // `#[path = "..."]`, which is relative to the declaring file.
    path: Option<String>,
    // Where its file is looked for otherwise.
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

// Takes a file's items apart: those of inline modules join the rest, and a
// module kept in another file is noted. `dir` is where this level's modules
// keep their files.
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

// Reads the file at `path`, whose own modules keep their files in `dir`,
// and then those modules.
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
        // A file named for its module keeps that module's own in a
        // directory of the same name; `mod.rs`, and a file given by path,
        // keep them beside themselves.
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

// Says which file an error was met in, if it does not say already.
fn located<T>(result: Result<T, Error>, source: &Source) -> Result<T, Error> {
    result.map_err(|mut error| {
        if error.file.is_none() {
            error.file.clone_from(&source.file);
        }
        error
    })
}

// The interfaces a file marks.
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
        // What the adapters name their own exports after.
        if ["Atomic", "Memory", "Future", "Stream", "Signal", "Outbox"].contains(&name.as_str()) {
            return error(
                item.ident.span(),
                format!("`{name}` is a name the adapters use; call the interface something else"),
            );
        }
        // Names are the library's, whatever module an item is in.
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
            methods: Vec::new(),
        });
    }
    Ok(())
}

// A shared interface's weak reference is a class of its name with `Weak`.
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
    Ok(())
}

// Every interface first, so that a block in one file can name one in another.
fn build(name: &str, sources: &[Source]) -> Result<Library, Error> {
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
    for source in sources {
        located(
            methods_of(&source.items, &mut interfaces, &names, &shared),
            source,
        )?;
    }
    Ok(Library {
        name: name.to_owned(),
        interfaces,
    })
}

/// Reads the marked items of `source`, the text of one Rust file, as the
/// API of the library `name`. Modules it keeps in other files are not read:
/// [`parse_crate`] reads a library from its files.
pub fn parse(name: &str, source: &str) -> Result<Library, Error> {
    let file = parse_text(source, None)?;
    let mut items = Vec::new();
    flatten(file.items, Path::new(""), &mut items, &mut Vec::new());
    build(name, &[Source { file: None, items }])
}

/// Reads the library `name` from the file at `root`, its `lib.rs`, and from
/// the file of every module declared from there down. Gives the files read
/// as well, for a build script to watch.
///
/// The adapters name an interface at the crate's root, so one defined in a
/// module is re-exported there.
pub fn parse_crate(name: &str, root: &Path) -> Result<(Library, Vec<PathBuf>), Error> {
    let mut sources = Vec::new();
    let mut files = Vec::new();
    let dir = root.parent().unwrap_or(Path::new(""));
    load(root, dir, &mut sources, &mut files)?;
    Ok((build(name, &sources)?, files))
}
