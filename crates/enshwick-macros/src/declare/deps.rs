//! `deps!()`: a module per dependency the crate's `Cargo.toml` names.

use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use super::cargo_metadata;

/// Expands `deps!()`: a module per dependency the crate's `Cargo.toml`
/// names by alias in `requires`, `after` or `recommends`, with the
/// dependency's id and its content ids, and the declarations
/// `cargo enshwick deps` wrote to `src/deps/<alias>.rs`, if any.
pub(crate) fn expand_deps() -> syn::Result<TokenStream> {
    let (metadata, shown) = cargo_metadata()?;
    let written = PathBuf::from(&shown)
        .parent()
        .map(|dir| dir.join("src").join("deps"));
    let modules = metadata.dependencies.iter().filter_map(|d| {
        let relation = match d.relation {
            "REQUIRES" => "requires",
            "RECOMMENDS" => "recommends",
            "AFTER" => "loads after",
            _ => return None,
        };
        let alias = Ident::new(&d.alias, Span::call_site());
        let id = &d.id;
        let doc = format!("`{id}`, which this mod {relation}.");
        let content = if d.relation == "REQUIRES" {
            quote! {
                /// The dependency's content id `<dependency>:<name>`, once
                /// the loader lets this mod name it: the dependency's
                /// `exports` list `<name>` or the name it is a part of.
                /// Ask once, at init, and keep the id.
                ///
                /// # Errors
                ///
                /// [`Off`](::enshwick::Off) when the dependency does not
                /// export the name; the loader logs what it exports.
                ///
                /// # Panics
                ///
                /// When `name` is not 1 to 128 of `[a-z0-9_./-]`.
                pub fn id(
                    name: &'static str,
                ) -> ::core::result::Result<::enshwick::content::Id, ::enshwick::Off> {
                    ::enshwick::__private::dependency_id(::enshwick::content::Id::of(ID, name))
                }
            }
        } else {
            quote! {
                /// The dependency's content id `<dependency>:<name>`; `None`
                /// while the dependency does not run, and when it does not
                /// export the name, which the loader logs. Each call asks
                /// the loader which mods run: call it once, at init, and
                /// keep the id.
                ///
                /// # Panics
                ///
                /// When `name` is not 1 to 128 of `[a-z0-9_./-]`, whether
                /// the dependency runs or not.
                #[must_use]
                pub fn id(name: &'static str) -> ::core::option::Option<::enshwick::content::Id> {
                    let id = ::enshwick::content::Id::of(ID, name);
                    if !::enshwick::mods::is_running(ID) {
                        return ::core::option::Option::None;
                    }
                    ::enshwick::__private::dependency_id(id).ok()
                }
            }
        };
        let declared = written
            .as_ref()
            .map(|dir| dir.join(format!("{}.rs", d.alias)))
            .filter(|path| path.is_file())
            .map(|path| {
                let path = path.display().to_string();
                quote! {
                    #[path = #path]
                    mod declared;
                    pub use self::declared::*;
                }
            });
        Some(quote! {
            #[doc = #doc]
            pub mod #alias {
                /// The dependency's mod id.
                pub const ID: &str = #id;

                #content

                #declared
            }
        })
    });
    Ok(quote! {
        /// The mods this one depends on, by the aliases its `Cargo.toml` gives
        /// them.
        pub mod deps {
            const _: &[u8] = ::core::include_bytes!(#shown);

            /// The game's own content, by the debug names its data gives it.
            pub mod vanilla {
                pub use ::enshwick::content::vanilla::{item, loot_table, recipe, station, workshop};
            }

            #(#modules)*
        }
    })
}
