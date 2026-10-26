# Texts

A mod ships its texts, item names, descriptions and chat phrases, in one
TOML file with a table per language. The
[`i18n`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
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
  [`Language`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
  lists the 35 the game knows, such as `en`, `en-us`, `de`, `uk`, `pt-br` or
  `zh-cn`. Codes ignore case; an unknown code is an error.
- Keys are the mod's own names for its texts; values must be strings.
- `en` is the fallback: a language without a text of its own for a key gets
  the `en` text.

Embed the file with `include_str!("texts.toml")` and read it during `init`
with
[`Load::texts`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs):

```rust
use enshwick::content::Id;

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

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
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

[`Load::add_texts(&texts, pairs)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
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
[`Texts`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
also serves the mod's own strings, such as chat replies:

- `get(key, language)` returns the text in `language`, else in `en`, else
  `None`;
- `fill(key, language, args)` returns the text with each `{name}` in it
  replaced by its value in `args`, or the key itself when no language has a
  text for it, so a missing text shows rather than vanishes;
- `translations(key)` lists every language with a text for the key;
- `merge(other)` adds `other`'s texts over these;
- `Texts::parse(toml)` reads a texts file without a `Load`, for tests.

```rust
use enshwick::i18n::{Language, Texts};

let mut texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n").unwrap();
assert_eq!(texts.get("greeting", Language::UK_UA), Some("Привіт"));
assert_eq!(texts.get("greeting", Language::DE_DE), Some("Hello"));

texts.merge(Texts::parse("[en]\ngreeting = \"Hi\"\n").unwrap());
assert_eq!(texts.get("greeting", Language::FALLBACK), Some("Hi"));
assert_eq!(Language::from_code("pt-BR"), Some(Language::PT_BR));
```

On a client,
[`i18n::language()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
is the language the player reads the game in, for the mod's own dialogs and
windows:

```rust
use enshwick::i18n::{Texts, language};

let texts = Texts::parse("[en]\nfound = \"{count} safes\"\n[uk]\nfound = \"Сейфів: {count}\"\n").unwrap();
let line = texts.fill("found", language(), &[("count", "3")]);
assert!(line.contains('3'));
```

The server does not know a player's language, and `language()` there is
`Language::FALLBACK`; an [action](../actions.md)'s request carries the
language the player's client reads.
[`enshwick-safes`](https://github.com/enshwick/mod-safes)
names a Safe's owner on its label in one language from its settings, falling
back to `Language::FALLBACK`.

## The game's own texts

A mod that names vanilla things, such as a survivor in its item's name,
reads the game's own text rather than shipping a copy of it, so the player
sees it in their language as the game spells it.
[`i18n::game_text(id)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
is the text of a tag in the language the player reads the game in, and
`game_text_in(id, language)` in any language; `id` is the tag's game id,
`Guid::id` of the tag a resource names. A text missing in a language comes
in English, as the game shows it.

```rust
use enshwick::content::Guid;
use enshwick::content::items::vanilla_name;
use enshwick::i18n::Language;
use enshwick::survivors::WorkshopSurvivor;

let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
let chest_name = vanilla_name(chest, Language::UK_UA);
let alchemist = WorkshopSurvivor::find("alchemist").unwrap();
let name = alchemist.name(Language::UK_UA).unwrap_or("Alchemist");
assert!(!name.is_empty() && chest_name.is_none_or(|n| !n.is_empty()));
```

`items::vanilla_name(item, language)` reads a vanilla item's name tag from
its data and returns its text; `WorkshopSurvivor::name(language)` is the
survivor's name, which the game gives its summoning staff.
[`enshwick-survivors-per-altar`](https://github.com/enshwick/mod-survivors-per-altar)
names its copy staffs this way: it registers each staff's name during
`init` for every language it translates, with the survivor's name in that
language.

The loader reads the texts from the game's archive files, never from the
game's memory, so they are there during `init` and on any thread. The first
call for a language reads that language's table, a few megabytes, and the
loader keeps it, so the texts it hands out stay valid for the process. Only
the client's archive holds texts: on a dedicated server every call returns
`None`, and a mod falls back to a name of its own there, which no player
sees.

## Overrides

A file `mod-configs/<id>.texts.toml`, in the same form as the mod's own,
overrides or adds texts: `load.texts` merges it over the embedded file. It
lets a server owner reword a mod or translate it into another language
without rebuilding it.

The loader's own texts, the words of the Mods screen, the answers of the
[console](../chat.md#the-console)'s tools, the reasons a settings change is refused and the
name of an item whose mod is gone, ship in English and Ukrainian in
[`texts.toml`](https://github.com/enshwick/enshwick/blob/main/crates/internal/loader/src/texts.toml),
and `mod-configs/enshwick.texts.toml` overrides them or adds a language the
same way.

The file is read on the machine the mod runs on. On a dedicated server it
changes the texts the mod uses there, such as its chat replies. Names and
descriptions a player sees come from the player's client, so they follow the
override file on that client.
