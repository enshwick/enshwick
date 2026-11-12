//! The edges through `Load`: a method call is no path, so a module reaching
//! a domain above by calling another module's `Load` method on `self` shows
//! only here. An inherent method's name is unique across all of `Load`'s
//! blocks, so the name alone tells its module.

use std::collections::BTreeSet;

use syn::Item;
use syn::visit::{self, Visit};

/// The methods `file`'s `impl Load` blocks add, and those their bodies call
/// on `self`; `#[cfg(test)]` items left out.
pub(super) fn load_methods(file: &syn::File) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut walk = LoadCalls::default();
    walk.visit_file(file);
    (walk.defined, walk.called)
}

/// The walk of [`load_methods`] over one file.
#[derive(Default)]
struct LoadCalls {
    /// Inside an inherent `impl Load`.
    in_load: bool,
    defined: BTreeSet<String>,
    called: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for LoadCalls {
    fn visit_item(&mut self, item: &'ast Item) {
        if !super::test_only(item) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let load = item.trait_.is_none()
            && matches!(&*item.self_ty, syn::Type::Path(ty)
                if ty.path.segments.last().is_some_and(|last| last.ident == "Load"));
        if load {
            for method in &item.items {
                if let syn::ImplItem::Fn(method) = method {
                    self.defined.insert(method.sig.ident.to_string());
                }
            }
        }
        let outer = std::mem::replace(&mut self.in_load, load);
        visit::visit_item_impl(self, item);
        self.in_load = outer;
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if self.in_load
            && matches!(&*call.receiver, syn::Expr::Path(receiver) if receiver.path.is_ident("self"))
        {
            self.called.insert(call.method.to_string());
        }
        visit::visit_expr_method_call(self, call);
    }
}
