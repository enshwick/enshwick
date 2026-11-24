//! What the layers above the engine keep of one loaded mod, a value per
//! type each layer owns, so the mod's record names no layer above it.

use core::any::Any;

/// What a layer keeps of one mod: a type of its own, which no other layer
/// reaches.
pub(crate) trait Fact: Any + Send + Sync + core::fmt::Debug {}

impl<T: Any + Send + Sync + core::fmt::Debug> Fact for T {}

/// The facts the parts made of one mod when its record was made, one per
/// type.
#[derive(Debug, Default)]
pub(crate) struct Facts(Box<[Box<dyn Fact>]>);

impl Facts {
    /// The facts `made`, the first of each type kept.
    pub(crate) fn new(made: Vec<Box<dyn Fact>>) -> Self {
        Self(made.into_boxed_slice())
    }

    /// The fact of type `T`; `None` when no part made one.
    pub(crate) fn get<T: Fact>(&self) -> Option<&T> {
        self.0
            .iter()
            .find_map(|fact| (&**fact as &dyn Any).downcast_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::Facts;

    #[derive(Debug, PartialEq)]
    struct Shown(u32);

    #[derive(Debug, PartialEq)]
    struct Packed(&'static str);

    #[test]
    fn each_type_reads_the_first_fact_made_of_it_and_none_without_one() {
        let facts = Facts::new(vec![
            Box::new(Shown(1)),
            Box::new(Packed("pack")),
            Box::new(Shown(2)),
        ]);
        assert_eq!(facts.get::<Shown>(), Some(&Shown(1)));
        assert_eq!(facts.get::<Packed>(), Some(&Packed("pack")));
        assert_eq!(facts.get::<u8>(), None);
        assert_eq!(Facts::default().get::<Shown>(), None);
    }
}
