### Changed

- Luau: a call into the API with a record, string or function argument
  costs less: a record's fields are read by interned names, a `&str`
  parameter's text lands in a buffer the runtime keeps, and a function
  argument is one allocation (#1063).
- Luau: a function the API hands a script raises its error as
  `function: <error>`, as its argument errors already did (#1063).
