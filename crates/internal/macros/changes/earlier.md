### Added

- Every attribute and derive macro takes the hidden argument
  `crate = "path"`, which names the crate its expansion reaches in place of
  `::enshwick`, for the API's internal crates (#1044).
