use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Meta, parse_macro_input};

/// 处理 `#[derive(Persistable)]` + `#[persistable(key = "...")]`。
pub(crate) fn expand_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let key = match parse_key_from_attrs(&input.attrs) {
        Some(k) => k,
        None => {
            return syn::Error::new_spanned(&input, r#"缺少 #[persistable(key = "...")] 属性"#)
                .to_compile_error()
                .into();
        }
    };
    TokenStream::from(emit(input.ident, &input.data, &key))
}

/// 生成 `impl Persistable` 和可能的 `impl Default`，返回 `TokenStream2`。
fn emit(ident: syn::Ident, data: &Data, key: &str) -> TokenStream2 {
    let mut expanded = quote! {
        impl ::models::Persistable for #ident {
            const KEY: &'static str = #key;
        }
    };

    if let Some(default_impl) = build_default(&ident, data) {
        expanded.extend(default_impl);
    }

    expanded
}

/// 从属性切片中提取 `#[persistable(key = "...")]` 的 key（derive 宏路径）。
fn parse_key_from_attrs(attrs: &[syn::Attribute]) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.path().is_ident("persistable"))
        .and_then(|a| a.parse_args::<Meta>().ok().and_then(extract_key))
}
fn extract_key(meta: Meta) -> Option<String> {
    if let Meta::NameValue(nv) = meta
        && let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = nv.value
    {
        return Some(s.value());
    }
    None
}

/// 若枚举的某个变体标注了 `#[default]`，生成对应的 `impl Default`。
fn build_default(name: &syn::Ident, data: &Data) -> Option<TokenStream2> {
    match data {
        Data::Enum(e) => {
            let default_variant = e
                .variants
                .iter()
                .find(|v| v.attrs.iter().any(|a| a.path().is_ident("default")))?;
            let variant_name = &default_variant.ident;
            Some(quote! {
                impl Default for #name {
                    fn default() -> Self {
                        Self::#variant_name
                    }
                }
            })
        }
        _ => None,
    }
}
