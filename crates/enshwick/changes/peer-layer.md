### Added

- `Actions::on_refused` hears each run of the mod's actions that the platform
  refused before the handler: who ran it, from where, the action, the text or
  values given and a typed `RefusedReason`, after the caller got the answer;
  Luau `actions:on_refused` (#1094).
- ABI 11.29: `ActionCall::REFUSED` tells a mod of a run the loader refused
  before asking it, with the appended `ActionCall::via` and
  `ActionCall::reason` and the `ActionCall::REFUSED_` reasons (#1094).
- `PlayerInfo::is_admin`, `PlayerInfo::admin`, `PlayerInfo::without_character`,
  `PlayerInfo::name_text` and `PlayerKey::from_hex` (#1094).
- Fake host: `Installed::refuse_run` tells the mod of a run the loader
  refused, as the loader does (#1094).

### Changed

- Loader: every way an action runs (a control, a panel, a window, chat, the
  console, a remote caller) passes one admission and one reply path. A
  player's runs from the interface and chat share one rate, 5 a second after
  a burst of 10; administrators, the console and remote callers are not
  rated; every refusal answers the caller with its reason, the first of a
  flood alone (#1094).
- Loader: a reply of several lines from a control or panel shows as one
  notice with its lines joined by ` / `, as a command's reply outside chat
  does (#1094).
- Loader: a settings change and a query over the rate of an administrator's
  client are refused by the same token bucket as every other rate, and only
  the first message of a flood over the query rate is answered busy (#1094).
- Fake host: an interface run with arguments out of the loader's bounds is
  refused before the mod, as the loader refuses it (#1094).

### Fixed

- Arguments from the interface that do not decode answer with the action's
  usage instead of "You may not do that." (#1094).
- `Action::admins` holds beside an `Action::allow` set before or after it;
  before, the later call replaced the earlier one (#1094).
- Loader: a run from a control or panel dropped for its rate, its bounds or
  an unknown mod id now answers the player instead of saying nothing (#1094).
