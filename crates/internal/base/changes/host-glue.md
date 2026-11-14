### Changed

- The helpers that read a mod's values and write the answers across the
  boundary live once here, for the loader and the fake host (#1083).
- A status the loader answers maps to each domain's error through one
  function, which also holds the rule for a status this build does not
  know (#1083).
