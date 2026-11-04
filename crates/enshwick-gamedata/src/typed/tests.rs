use super::*;
use crate::reflect::{Image, Layout};

#[test]
fn the_relevance_rule_reads() {
    let relevance = Relevance::read();
    assert_eq!(
        relevance.roots.len() + relevance.suffixes.len(),
        Role::ALL.len()
    );
}

/// The catalogue of the server executable `ENSH_SERVER_EXE` names, when it
/// names one of `age`; `None` to skip.
fn server(age: u32) -> Option<Catalogue> {
    let path = std::env::var("ENSH_SERVER_EXE").ok()?;
    let bytes = std::fs::read(path).unwrap();
    let image = Image::parse(&bytes, Layout::File, None).unwrap();
    (image.fingerprint()?.age() == age)
        .then(|| Catalogue::from_reflection(&Reflection::scan(&image)))
}

/// The API coverage count was made on server build 387: 648 components,
/// 127 events and 211 resources.
#[test]
fn build_387_covers_the_counted_types() {
    let Some(catalogue) = server(387) else {
        return;
    };
    let counts = Role::ALL.map(|role| catalogue.count(role));
    assert_eq!(counts, [648, 127, 211]);
}
