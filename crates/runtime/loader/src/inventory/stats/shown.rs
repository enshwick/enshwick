//! How stat kinds show to a player: their names and descriptions, the rows
//! of the Character screen's groups, and the lines an item's modifiers and
//! affixes add to its tooltip.

use enshwick_core::abi::StatModifier;
use enshwick_core::content::items::StatFormat;
use enshwick_core::i18n::Language;
use enshwick_core::stats::{StatGroup, description_key};

use super::SEALED;
use super::own::shown;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// The kinds whose rows show in the Character screen's `group`, by index in
/// definition order.
pub(crate) fn group_kinds(group: StatGroup) -> Vec<u32> {
    SEALED.get().map_or_else(Vec::new, |sealed| {
        (0..sealed.kinds.len())
            .filter(|&index| sealed.defs[index].group == Some(group) && shown(sealed, index))
            .filter_map(|index| u32::try_from(index).ok())
            .collect()
    })
}

/// How a kind shows to a player: its name, its description (empty for
/// none) and how its value reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShownKind {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) format: StatFormat,
}

/// How kind `index` shows in `language`; `None` for a kind no one defines,
/// a hidden one, or one without a name text. A built-in kind takes its name
/// from the loader's texts, a mod's from that mod's.
pub(crate) fn shown_kind(loader: &Loader, index: u32, language: Language) -> Option<ShownKind> {
    let sealed = SEALED.get()?;
    let kind = sealed.kinds.get(usize::try_from(index).ok()?)?;
    let (def, name_key, owner) = kind.def.as_ref()?;
    if def.hidden {
        return None;
    }
    let description_key = description_key(&kind.id).unwrap_or_default();
    let (name, description) = if *owner == ModIndex::LOADER.get() {
        (crate::engine::texts::fill(name_key, &[]), String::new())
    } else {
        let texts = loader.mods.get(*owner)?.texts.relock();
        let texts = texts.as_ref()?;
        (
            texts.get(name_key, language)?.to_owned(),
            texts
                .get(&description_key, language)
                .unwrap_or_default()
                .to_owned(),
        )
    };
    (!name.is_empty() && name != **name_key).then_some(ShownKind {
        name,
        description,
        format: def.format,
    })
}

/// The tooltip lines of the item `pide` holding `item`: one per modifier of
/// its own and of each affix it rolled, `+10% Luck`, in `language`; none
/// for a hidden kind or one without a name.
pub(crate) fn item_line_texts(
    loader: &Loader,
    pide: u32,
    item: u32,
    language: Language,
) -> Vec<String> {
    let Ok(lines) = super::item_lines(pide, item) else {
        return Vec::new();
    };
    lines
        .iter()
        .filter_map(|line| {
            let shown = shown_kind(loader, line.modifier.stat, language)?;
            let value = modifier_value(line.modifier.op, line.modifier.value, shown.format);
            Some(format!("{value} {}", shown.name))
        })
        .collect()
}

/// A modifier's value as its line reads: an addition with its sign, a
/// factor as the change it makes in percent, a floor or a set value as the
/// kind's format reads it.
fn modifier_value(op: u8, value: f32, format: StatFormat) -> String {
    match (op, format) {
        (StatModifier::MULTIPLY, _) => format!("{}%", signed(number((value - 1.0) * 100.0))),
        (StatModifier::ADD, StatFormat::Percent) => format!("{}%", signed(number(value * 100.0))),
        (StatModifier::ADD, _) => signed(number(value)),
        (_, StatFormat::Percent) => format!("{}%", number(value * 100.0)),
        _ => number(value),
    }
}

/// `value` with at most two decimals and no trailing zeros.
fn number(value: f32) -> String {
    let text = format!("{:.2}", (value * 100.0).round() / 100.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// `number` with a `+` in front when it is above 0.
fn signed(number: String) -> String {
    if number.starts_with('-') || number == "0" {
        number
    } else {
        format!("+{number}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modifier_reads_as_its_op_and_its_kind_s_format_ask() {
        let cases = [
            (StatModifier::ADD, 3.0, StatFormat::Number, "+3"),
            (StatModifier::ADD, -1.5, StatFormat::Signed, "-1.5"),
            (StatModifier::ADD, 0.1, StatFormat::Percent, "+10%"),
            (StatModifier::MULTIPLY, 1.25, StatFormat::Number, "+25%"),
            (StatModifier::MULTIPLY, 0.8, StatFormat::Percent, "-20%"),
            (StatModifier::MAX, 0.5, StatFormat::Percent, "50%"),
            (StatModifier::OVERRIDE, 2.0, StatFormat::Number, "2"),
            (StatModifier::ADD, 0.0, StatFormat::Number, "0"),
            (StatModifier::ADD, 1.0 / 3.0, StatFormat::Number, "+0.33"),
        ];
        for (op, value, format, read) in cases {
            assert_eq!(modifier_value(op, value, format), read, "{op} {value}");
        }
    }
}
