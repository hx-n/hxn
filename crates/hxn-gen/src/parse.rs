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

fn param_type(ty: &syn::Type) -> Result<Type, Error> {
    if let syn::Type::Reference(reference) = ty
        && reference.mutability.is_none()
    {
        if path_name(&reference.elem).as_deref() == Some("str") {
            return Ok(Type::Str);
        }
        if let syn::Type::Slice(slice) = &*reference.elem
            && path_name(&slice.elem).as_deref() == Some("u8")
        {
            return Ok(Type::Bytes);
        }
    }
    match path_name(ty).as_deref() {
        Some("bool") => Ok(Type::Bool),
        Some("i32") => Ok(Type::I32),
        Some("f64") => Ok(Type::F64),
        _ => error(
            ty.span(),
            "a parameter is one of bool, i32, f64, &str or &[u8]",
        ),
    }
}

fn result_type(ty: &syn::Type, interface: &str) -> Result<Type, Error> {
    if let syn::Type::Tuple(tuple) = ty
        && tuple.elems.is_empty()
    {
        return Ok(Type::Void);
    }
    match path_name(ty).as_deref() {
        Some("bool") => Ok(Type::Bool),
        Some("i32") => Ok(Type::I32),
        Some("f64") => Ok(Type::F64),
        Some("String") => Ok(Type::Str),
        Some(name) if name == "Self" || name == interface => Ok(Type::Own),
        _ => error(
            ty.span(),
            "a result is one of (), bool, i32, f64, String or Self, or a Result of one",
        ),
    }
}

// `Result<T, E>` gives `T` and says the function can fail.
fn returns(output: &ReturnType, interface: &str) -> Result<(Type, bool), Error> {
    let ReturnType::Type(_, ty) = output else {
        return Ok((Type::Void, false));
    };
    if let syn::Type::Path(path) = &**ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Result"
        && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
        && let Some(syn::GenericArgument::Type(ok)) = arguments.args.first()
    {
        return Ok((result_type(ok, interface)?, true));
    }
    Ok((result_type(ty, interface)?, false))
}

fn method(function: &syn::ImplItemFn, interface: &str) -> Result<Method, Error> {
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
                receiver = match (&this.reference, &this.mutability) {
                    (Some(_), None) => Receiver::Shared,
                    (Some(_), Some(_)) => Receiver::Exclusive,
                    (None, _) => {
                        return error(this.span(), "a method takes `&self` or `&mut self`");
                    }
                };
            }
            FnArg::Typed(typed) => {
                let syn::Pat::Ident(pattern) = &*typed.pat else {
                    return error(typed.pat.span(), "a parameter needs a plain name");
                };
                params.push(Param {
                    name: pattern.ident.to_string(),
                    ty: param_type(&typed.ty)?,
                });
            }
        }
    }

    let (returns, fallible) = returns(&signature.output, interface)?;
    if returns == Type::Own && (receiver != Receiver::None || name != "new") {
        return error(
            signature.output.span(),
            "only `new` returns the interface itself",
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
    })
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
        interfaces.push(Interface {
            name: item.ident.to_string(),
            docs: docs(&item.attrs),
            methods: Vec::new(),
        });
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
        for item in &block.items {
            let ImplItem::Fn(function) = item else {
                continue;
            };
            if !matches!(function.vis, Visibility::Public(_)) {
                continue;
            }
            let method = method(function, &target)?;
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
