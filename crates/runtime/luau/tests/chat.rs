//! `enshwick.chat`: the chat filter and the lines a script hears, as a Rust
//! mod's.

#[cfg(test)]
mod tests {
    use enshwick::PlayerKey;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_drops_rewrites_and_keeps_lines_and_hears_what_goes_on() {
        let loader = run(
            FakeHost::new().world(),
            r#"
            enshwick.chat.on_chat(function(line)
                if line.text == "drop me" then
                    return false
                elseif line.kind == "Player" and line.text:find("bad") then
                    return (line.text:gsub("bad", "good"))
                end
                return nil
            end)
            enshwick.chat.on_chat_said(function(line)
                enshwick.log.info(`heard {line.kind} {line.text}`)
            end)
            "#,
        );
        let ann = PlayerKey::new(7);
        assert_eq!(loader.say(ann, "a bad day").as_deref(), Some("a good day"));
        assert_eq!(loader.say(ann, "drop me"), None);
        assert_eq!(loader.say(ann, "hello").as_deref(), Some("hello"));
        assert!(
            logged(&loader, "heard Player a good day"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "heard Player hello"));
        assert!(!logged(&loader, "drop me"), "a dropped line is not heard");
    }
}
