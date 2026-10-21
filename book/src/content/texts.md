# Texts

A mod ships its texts, item names, descriptions and chat phrases, in one
TOML file with a table per language. The
[`i18n`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/i18n.rs)
module reads it, and a server owner can add to it or change it.

## The texts file

```toml
[en]
"safe.name" = "Safe"
"safe.description" = "A chest only its owner can open."

[uk]
"safe.name" = "Сейф"

[de]
"safe.name" = "Tresor"
```

- Each table is a language code:
  [`Language`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/i18n.rs)
  lists the 35 the game knows, such as `en`, `en-us`, `de`, `uk`, `pt-br` or
  `zh-cn`. Codes ignore case; an unknown code is an error.
- Keys are the mod's own names for its texts; values must be strings.
- `en` is the fallback: a language without a text of its own for a key gets
  the `en` text.

Embed the file with `include_str!("texts.toml")` and read it during `init`
with
[`Load::texts`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/i18n.rs):

```rust
use enshwick_loader_api::content::Id;

const SAFE_NAME: Id = Id::new("safes:safe.name");
const SAFE_DESCRIPTION: Id = Id::new("safes:safe.description");

// In a mod: const TEXTS: &str = include_str!("texts.toml");
const TEXTS: &str = r#"
[en]
"safe.name" = "Safe"
"safe.description" = "A chest only its owner can open."

[uk]
"safe.name" = "Сейф"
"#;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let texts = load.texts(TEXTS)?;
    load.add_texts(&texts, &[
        ("safe.name", SAFE_NAME),
        ("safe.description", SAFE_DESCRIPTION),
    ])
}
```

`load.texts` returns `Err(Off)` when the embedded file or the override file
below is malformed, with the file's path in the message for the override.

## Texts the game shows

[`Load::add_texts(&texts, pairs)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/i18n.rs)
gives game tags their texts. Each pair is a key of the texts file and a tag,
a [content id](../content.md#content-ids) of the mod's own, whose GUID goes
into the `name` and `description` of a [`NewItem`](items.md). Every language that has a text for the key gets it;
a key with no text in any language is an error.

The client shows the texts: wherever the game asks for the tag, it gets the
text in the player's language, else the `en` text. The dedicated server
shows no text, so there the call has no visible effect; call it anyway, the
same code runs on both sides.

Texts are registered during `init` only; the loader refuses them later.
Under the hood each text goes through
[`add_text`](resources.md#one-text-for-one-tag).

## Texts the mod uses itself

The returned
[`Texts`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/i18n.rs)
also serves the mod's own strings, such as chat replies:

- `get(key, language)` returns the text in `language`, else in `en`, else
  `None`;
- `translations(key)` lists every language with a text for the key;
- `merge(other)` adds `other`'s texts over these;
- `Texts::parse(toml)` reads a texts file without a `Load`, for tests.

```rust
use enshwick_loader_api::i18n::{Language, Texts};

let mut texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n").unwrap();
assert_eq!(texts.get("greeting", Language::UK_UA), Some("Привіт"));
assert_eq!(texts.get("greeting", Language::DE_DE), Some("Hello"));

texts.merge(Texts::parse("[en]\ngreeting = \"Hi\"\n").unwrap());
assert_eq!(texts.get("greeting", Language::FALLBACK), Some("Hi"));
assert_eq!(Language::from_code("pt-BR"), Some(Language::PT_BR));
```

The server does not know a player's language. [`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
answers its `/safe` command in one language from its settings, falling back
to `Language::FALLBACK`.

## Overrides

A file `mod-configs/<id>.texts.toml`, in the same form as the mod's own,
overrides or adds texts: `load.texts` merges it over the embedded file. It
lets a server owner reword a mod or translate it into another language
without rebuilding it.

The file is read on the machine the mod runs on. On a dedicated server it
changes the texts the mod uses there, such as its chat replies. Names and
descriptions a player sees come from the player's client, so they follow the
override file on that client.
