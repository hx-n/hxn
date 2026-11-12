//! The attribute macros a library uses to mark its API.
//!
//! They generate no code. `hxn-gen` reads them from the source and writes
//! each runtime's adapter into a separate crate, which a macro expanding in
//! the library's crate could not do. The macros exist so that the markers
//! compile: each returns its item unchanged, except that it removes the
//! `#[hxn(...)]` attributes inside it, which no other macro would consume.

use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

/// Marks a struct as a value Haxe holds by handle. It becomes a class with the
/// methods of its `#[hxn::methods]` blocks, released by `close()` or the
/// garbage collector.
#[proc_macro_attribute]
pub fn interface(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

/// Marks an `impl` block whose public functions are exposed. A function may
/// carry `#[hxn(blocking)]` to declare that it blocks.
#[proc_macro_attribute]
pub fn methods(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

/// Marks a public function outside an `impl` as a static function of a Haxe
/// class: by default the class named after the library, or the class that
/// `class = "Name"` specifies.
#[proc_macro_attribute]
pub fn function(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

/// Marks a struct or an enum as plain data, which is passed by value. A
/// struct's public fields become the fields of a Haxe class. A C-like enum
/// becomes a Haxe `enum abstract` with the same names.
#[proc_macro_attribute]
pub fn data(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    without_notes(item)
}

// `#[hxn(...)]` is a `#` followed by a bracket group that starts with `hxn (`.
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
