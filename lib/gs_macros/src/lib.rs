//! Custom proc macros Geosia uses

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{ExprRepeat, Ident, LitInt, Result, Token, TypeArray, parse::{Parse, ParseStream}, parse_macro_input, token};


enum NestedElement {
    Init(ExprRepeat),
    Type(TypeArray),
}

impl Parse for NestedElement {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Token![let]) {
            input.parse::<Token![let]>()?;
            Ok(NestedElement::Init(input.parse()?))
        } else if lookahead.peek(token::Bracket) {
            Ok(NestedElement::Type(input.parse()?))
        } else {
            Err(lookahead.error())
        }
    }
}

struct Nest {
    depth: usize,
    element: NestedElement,
}

impl Parse for Nest {
    fn parse(input: ParseStream) -> Result<Self> {
        let depth = input.parse::<LitInt>()?.base10_parse()?;
        input.parse::<token::Comma>()?;
        let element = input.parse::<NestedElement>()?;

        Ok(Nest { depth, element })
    }
}

/// A proc macro for 'nesting' an array type or initializer in itself a (constant, but user-definable) amount of times.
///
/// Usage:
///
///     nest!($DEPTH, [$TYPE; $SIZE])
///
///     nest!($DEPTH, let [$DEFAULT; $SIZE])
///
/// # Examples
///
/// ```
/// # use gs_macros::make_nested_array;
///
/// make_nested_array!(3, [T; 4]);
/// // -> [[[T; 4]; 4]; 4]
/// make_nested_array!(5, [T; 4]);
/// // -> [[[[[T; 4]; 4]; 4]; 4]; 4]
/// make_nested_array!(3, let [T::default(); 4]);
/// // -> [[[T::default(); 4]; 4]; 4]
/// ```
#[proc_macro]
pub fn make_nested_array(input: TokenStream) -> TokenStream {
    let Nest { depth, element } = parse_macro_input!(input as Nest);

    let expanded = match element {
        NestedElement::Type(ty) => {
            let TypeArray { elem, len, .. } = ty;
            to_nested_array(quote! { #elem }, quote! { #len }, depth)
        },
        NestedElement::Init(expr) => {
            let ExprRepeat { expr, len, .. } = expr;
            to_nested_array(quote! { #expr }, quote! { #len }, depth)
        }
    };

    TokenStream::from(expanded)
}

struct AllArrays {
    macro_ident: Ident,
    start: usize,
    end: usize,
    array_type: TypeArray,
    idents: Vec<Ident>,
}

impl Parse for AllArrays {
    fn parse(input: ParseStream) -> Result<Self> {
        let macro_ident = input.parse::<Ident>()?;
        input.parse::<token::Comma>()?;
        let start = input.parse::<LitInt>()?.base10_parse()?;
        input.parse::<token::Comma>()?;
        let end = input.parse::<LitInt>()?.base10_parse()?;
        input.parse::<token::Comma>()?;
        let array_type = input.parse::<TypeArray>()?;
        input.parse::<token::Comma>()?;
        let mut idents = vec![input.parse::<Ident>()?];
        while input.parse::<token::Comma>().is_ok() {
            idents.push(input.parse::<Ident>()?);
        }

        Ok(AllArrays {
            macro_ident,
            start,
            end,
            array_type,
            idents,
        })
    }
}

/// Helper macro to generate nested arrays. Useful to generate scaffolding to work around Rust lacking variadics.
/// Invoking `all_args!(impl_foo, start, end, [T; N])` invokes `impl_foo` providing functions accepting nested arrays
/// through arity `start..=end`.
///
/// # Examples
///
/// ```
/// # use gs_macros::all_arrays;
///
/// macro_rules! n_foo {
///     ($n:literal, $arr:expr, $name:ident) => {
///         pub fn $name<T>(n: $arr, alpha: T) -> T {
///             for n in 0..$n {
///
///             }
///             T::default()
///         }
///     }
/// }
///
/// all_arrays!(n_foo, 1, 5, [T; 4], depth_);
/// // n_foo!(1, [T; 4], depth_1);
/// // n_foo!(2, [[T; 4]; 4], depth_1, depth_2);
/// // n_foo!(3, [[[T; 4]; 4]; 4], depth_1, depth_2, depth_3);
/// // n_foo!(4, [[[[T; 4]; 4]; 4]; 4], depth_1, depth_2, depth_3, depth_4);
/// // n_foo!(5, [[[[[T; 4]; 4]; 4]; 4]; 4], depth_1, depth_2, depth_3, depth_4, depth_5);
/// ```
#[proc_macro]
pub fn all_arrays(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as AllArrays);
    let len = 2 + input.end - input.start;
    let mut ident_tuples = Vec::with_capacity(len);
    for i in 0..=len {
        let idents = input
            .idents
            .iter()
            .map(|ident| format_ident!("{}{}", ident, i + input.start - 2));
        ident_tuples.push(to_ident_tuple(idents, input.idents.len()));
    }

    let macro_ident = &input.macro_ident;
    let array_type = &input.array_type;
    let invocations = (input.start..=input.end).map(|i| {
        let ident_idx = 2 + i - input.start;
        let template_name = format_ident!("{}{}", input.idents[0], i);
        let ident_tuples = &ident_tuples[..ident_idx];

        let array_type = {
            let array_len = &array_type.len;
            let array_type = &array_type.elem;
            to_nested_array(quote! { #array_type }, quote! { #array_len }, i)
        };

        quote! {
            #macro_ident!(#template_name #i #array_type #(#ident_tuples),*);
        }
    });
    TokenStream::from(quote! {
        #(
            #invocations
        )*
    })
}

fn to_nested_array(array_val: TokenStream2, array_len: TokenStream2, depth: usize) -> TokenStream2 {
    let mut expanded = array_val;
    for _ in 0..depth {
        expanded = quote! {
            [#expanded; #array_len]
        };
    }
    expanded
}

fn to_ident_tuple(idents: impl Iterator<Item = Ident>, len: usize) -> TokenStream2 {
    if len < 2 {
        quote! { #(#idents)* }
    } else {
        quote! { (#(#idents),*) }
    }
}
