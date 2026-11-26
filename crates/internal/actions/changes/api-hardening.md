### Changed

- **Breaking:** `on_craft` and `on_build` take a filter that answers
  anything `Into<Verdict>`; `BuildVerdict` is gone (#1295).
- **Breaking:** `on_chat` takes a filter that answers anything
  `Into<ChatVerdict>`, which a `bool` and a `Verdict` convert to (#1295).
- **Breaking:** `console` is stable and its error is `RunLineError` (#1295).
