extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemFn};

/// Attribute macro that makes any Rust function both callable as normal *and*
/// introspectable by generating a hidden “_as_string” helper that returns
/// its own source code as a `&'static str`.
///
/// # What it does
///
/// When you write:
/// ```rust
/// use ai_macros::ai_function;
///
/// #[ai_function]
/// fn foo(x: i32) -> i32 {
///     x + 1
/// }
/// ```
///
/// this macro will expand to:
///
/// 1. **The original** `fn foo(x: i32) -> i32 { x + 1 }` exactly as you wrote it,  
/// 2. **A hidden helper**  
///    ```rust
///    #[doc(hidden)]
///    #[allow(dead_code)]
///    pub fn foo_as_string() -> &'static str {
///        stringify!(fn foo(x: i32) -> i32 { x + 1 })
///    }
///    ```
///
/// so you can still call `foo(5)` and get `6`, **and** call `foo_as_string()`
/// to retrieve its source for sending to an AI or other tooling.
///
/// # How it works
///
/// 1. **Clone the raw tokens** (`item.clone()`) so we can both re‐emit them verbatim  
/// 2. **Parse** into a `syn::ItemFn` to extract `vis`, `ident`, etc.  
/// 3. **Compute** a new helper name by appending `"_as_string"` to your function’s name  
/// 4. **`quote!`-generate** two things:
///    - the original function tokens (so its behavior is unchanged),  
///    - a hidden, `dead_code`‐allowed function that calls `stringify!` on those tokens,
///      yielding a compile‐time `&'static str`.
#[proc_macro_attribute]
pub fn ai_function(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Convert the incoming TokenStream into proc_macro2 for quoting:
    let original_ts: TokenStream2 = item.clone().into();

    // Parse into a syn::ItemFn so we can grab `vis` and `ident`:
    let input_fn: ItemFn = parse_macro_input!(item as ItemFn);
    let vis = &input_fn.vis;
    let name = &input_fn.sig.ident;
    let helper = format_ident!("{}_as_string", name, span = name.span());

    // Re-emit the original fn + hidden `*_as_string` helper:
    let expanded = quote! {
        #original_ts

        #[doc(hidden)]
        #[allow(dead_code)]
        #vis fn #helper() -> &'static str {
            stringify!(#original_ts)
        }
    };

    // Convert *back* into the compiler’s TokenStream:
    TokenStream::from(expanded)
}
