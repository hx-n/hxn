//! The attributes a library marks its API with.
//!
//! They generate nothing. `hxn-gen` reads them from the source and writes
//! each runtime's adapter into a crate of its own, which a macro expanding
//! here could not do. What these do is make the markers legal Rust: they
//! hand the item back as it was, less the `#[hxn(...)]` notes inside it,
//! which no macro would otherwise answer to.

use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

/// Marks a struct as a value Haxe holds: a class with the methods of its
/// `#[hxn::methods]` blocks, released by `close()` or the collector.
#[proc_macro_attribute]
pub fn interface(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

/// Marks an `impl` block whose public functions are exposed. A function may
/// carry `#[hxn(blocking)]` to say it waits.
#[proc_macro_attribute]
pub fn methods(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

/// Marks a public function outside an `impl` as a static function of a Haxe
/// class: the one named after the library, or `class = "Name"`.
#[proc_macro_attribute]
pub fn function(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

// `#[hxn(...)]` is a `#` and then a bracket group that starts `hxn (`.
fn is_note(group: &Group) -> bool {
    if group.delimiter() != Delimiter::Bracket {
        return false;
    }
    let mut tokens = group.stream().into_iter();
    matches!(tokens.next(), Some(TokenTree::Ident(name)) if name.to_string() == "hxn")
        && matches!(tokens.next(), Some(TokenTree::Group(arguments)) if arguments.delimiter() == Delimiter::Parenthesis)
}

fn without_notes(stream: TokenStream) -> TokenStream {
    let mut kept = Vec::new();
    let mut tokens = stream.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Punct(hash) if hash.as_char() == '#' => {
                if matches!(tokens.peek(), Some(TokenTree::Group(group)) if is_note(group)) {
                    tokens.next();
                } else {
                    kept.push(TokenTree::Punct(hash));
                }
            }
            TokenTree::Group(group) => {
                let mut inner = Group::new(group.delimiter(), without_notes(group.stream()));
                inner.set_span(group.span());
                kept.push(TokenTree::Group(inner));
            }
            other => kept.push(other),
        }
    }
    kept.into_iter().collect()
}
