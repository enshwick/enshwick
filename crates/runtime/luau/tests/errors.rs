//! A call into the API that fails raises a table of `kind`, `message` and
//! `call`, on every path a script reaches it by: start-time functions, a
//! namespace's getter and the functions it opens.

#[cfg(test)]
mod tests {
    use enshwick::events::WorldSaved;
    use enshwick::script::Error;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    /// Logs `label` with what a failed `pcall` of `call` caught: its type, and
    /// for a table its kind and how `tostring` reads it.
    const CAUGHT: &str = r#"
        local function caught(label, call, ...)
            local ok, why = pcall(call, ...)
            if type(why) == "table" then
                enshwick.log.info(`{label} {ok} table {why.kind} {why.call} | {tostring(why)}`)
            else
                enshwick.log.info(`{label} {ok} {type(why)}`)
            end
        end
    "#;

    #[test]
    fn a_start_call_raises_a_table_while_the_mod_starts_and_after() {
        let main = format!(
            r#"{CAUGHT}
            caught("bad", enshwick.vetoes.on_craft, 5)
            caught("early", enshwick.altars.list)
            local altars = enshwick.altars.altars()
            enshwick.on("WorldSaved", function()
                caught("late", enshwick.vetoes.on_craft, function() end)
                caught("again", enshwick.altars.altars)
            end)
            "#
        );
        let loader = run(FakeHost::new(), &main);
        let _handled = loader.raise::<WorldSaved>(&());
        let bad = format!(
            "bad false table {} enshwick.vetoes.on_craft | ",
            Error::BAD_ARGS
        );
        let early = format!(
            "early false table {} enshwick.altars.list | enshwick.altars.list: call enshwick.altars.altars() at the top level of main.luau first",
            Error::WRONG_TIME
        );
        let late = format!(
            "late false table {} enshwick.vetoes.on_craft | enshwick.vetoes.on_craft: allowed only while the mod starts",
            Error::WRONG_TIME
        );
        let again = format!(
            "again false table {} enshwick.altars.altars | ",
            Error::WRONG_TIME
        );
        for line in [bad, early, late, again] {
            assert!(logged(&loader, &line), "{line}\n{:?}", loader.logs());
        }
    }
}
