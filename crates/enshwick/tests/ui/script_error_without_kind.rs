//! A `#[script]` method's error names its kinds: one without `ErrorKind`
//! does not compile.

#[derive(Debug)]
struct NoSeed;

impl core::fmt::Display for NoSeed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("no seed")
    }
}

struct Garden;

#[enshwick::script(garden)]
impl Garden {
    /// Plants a seed, if there is one.
    #[script]
    pub fn plant(&self) -> Result<u32, NoSeed> {
        Err(NoSeed)
    }
}

fn main() {
    let _ = Garden::SCRIPT_GARDEN;
}
