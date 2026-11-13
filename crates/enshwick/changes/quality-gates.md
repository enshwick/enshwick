### Fixed

- Docs: the script impls of the API's types show the `unstable` badge again
  on docs.rs (#1077).

- Test fake: `Knowledge::revoke`, `revoke_world`, `grant_account` and
  `revoke_account` answer as the loader does. An unknown id or a change of the
  wrong kind is refused, and so is an account change of a world id (#1077).
