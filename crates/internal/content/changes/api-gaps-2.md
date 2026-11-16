### Changed

- The content helpers build into an output passed as their last argument,
  `Blob` edits in a `Vec<u8>` or, through `Blob::over`, in a served
  resource's `Output`, and `Blob::grow_array` moves an array in place
  (#1130).
