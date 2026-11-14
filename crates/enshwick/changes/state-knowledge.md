### Fixed

- Forget the knowledge changes waiting for accounts, the revokes not yet raised, the last knowledge update seen and the knowledge guard's refusal count when a client's hosted game ends, so the next world a client hosts starts from its own (#1093).
- Answer a skill check, comfort or experience filter registered after the loader sealed them with `TOO_LATE`; one that raced the seal was dropped with `OK` (#1093).
