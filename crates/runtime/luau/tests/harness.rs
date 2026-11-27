//! A Luau mod with the scripts a test gives, started on a fake host: the
//! one harness of the tests of each API module's Luau calls.

#![cfg(test)]

use std::sync::Mutex;
use std::time::Duration;

use enshwick::abi::Status;
use enshwick::config::Schema;
use enshwick::test::{FakeHost, Installed};
use enshwick::{Load, Mod, Off};
use enshwick_luau::{Limits, Runtime, Scripts};

static CASE: Mutex<Option<(Scripts, Option<Schema>)>> = Mutex::new(None);
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

struct Luau;

impl Mod for Luau {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let (scripts, settings) = CASE.lock().unwrap().take().unwrap();
        let runtime = enshwick_luau::start(load, "you.luau", scripts, settings, untimed())?;
        *RUNTIME.lock().unwrap() = Some(runtime);
        Ok(Self)
    }
}

enshwick::test::declare_mod!(
    Luau,
    r#"
    id = "you.luau"
    side = "both"
    "#
);

/// Starts `scripts` with `main` as the mod's `main.luau` and the settings
/// `settings` declares on `host`, with the status init returned.
pub(crate) fn start_with(
    host: FakeHost,
    main: &str,
    scripts: Scripts,
    settings: Option<Schema>,
) -> (Installed, Status) {
    *CASE.lock().unwrap() = Some((scripts.with("main.luau", main), settings));
    let loader = host.install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    (loader, status)
}

/// The default limits with time budgets no preemption on a busy test machine
/// reaches: a call held past the default 10 ms would turn the mod off and fail
/// a test that does not measure time, while a script that loops by mistake
/// still fails within seconds.
pub(crate) fn untimed() -> Limits {
    let mut limits = Limits::default()
        .with_call(UNTIMED_BUDGET)
        .with_per_second(Duration::from_secs(3_600));
    limits.start = UNTIMED_BUDGET;
    limits
}

/// The call and start budget of [`untimed`].
const UNTIMED_BUDGET: Duration = Duration::from_secs(5);

/// Starts `main` as the mod's `main.luau` on `host`, with the status init
/// returned.
pub(crate) fn start(host: FakeHost, main: &str) -> (Installed, Status) {
    start_with(host, main, Scripts::new(), None)
}

/// Starts `main` on `host` and expects the start to succeed.
pub(crate) fn run(host: FakeHost, main: &str) -> Installed {
    let (loader, status) = start(host, main);
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    loader
}

/// Whether a log line of `loader` holds `text`.
pub(crate) fn logged(loader: &Installed, text: &str) -> bool {
    loader.logs().iter().any(|(_, line)| line.contains(text))
}

/// An action appended to a script that declares `actions`: it shows its
/// runner a window that draws the action its text names.
pub(crate) const LOOK: &str = r#"
    actions:add({ id = "look", label = "Look", args = { enshwick.actions.text("Action", 32) } },
        function(request)
            local window = { id = "look", title = "Look", items = { { Action = request:args()[1].Text } } }
            actions:show(window, { request:player().key })
            return enshwick.actions.done("")
        end)
"#;

/// The entry of action `id` that the script's [`LOOK`] window shows
/// `viewer` in `language`.
pub(crate) fn looked(
    loader: &Installed,
    viewer: &enshwick::abi::PlayerInfo,
    language: enshwick::i18n::Language,
    id: &str,
) -> enshwick::actions::sheet::Entry {
    let shown = loader.ask_actions(
        enshwick::abi::ActionCall::RUN,
        viewer,
        0,
        "look",
        &format!("t{id}"),
    );
    assert_eq!(shown.as_deref(), Some("1"), "{:?}", loader.logs());
    let carried = loader.carried_windows(viewer, language.0);
    let (_, text) = carried.iter().find(|(window, _)| window == "look").unwrap();
    enshwick::actions::window::decode(text)
        .unwrap()
        .entries
        .remove(0)
}

mod tests {
    use enshwick::events::WorldSaved;
    use enshwick::test::FakeHost;
    use enshwick_luau::Limits;

    use super::{run, untimed};

    #[test]
    fn calls_a_busy_machine_holds_past_the_default_budgets_keep_the_test_mod_on() {
        let loader = run(
            FakeHost::new(),
            r#"enshwick.on("WorldSaved", function()
                local start = os.clock()
                while os.clock() - start < 0.03 do end
            end)"#,
        );
        // Four 30 ms calls pass both the default 10 ms per call and 100 ms per second.
        for _ in 0..4 {
            let _handled = loader.raise::<WorldSaved>(&());
        }
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }

    #[test]
    fn the_untimed_limits_lift_every_default_time_budget() {
        let (untimed, default) = (untimed(), Limits::default());
        assert!(untimed.call > default.call);
        assert!(untimed.per_second > default.per_second);
        assert!(untimed.start > default.start);
    }
}
