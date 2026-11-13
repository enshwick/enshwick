### Fixed

- Test fake: `Load::on_survivor_status` answers as the loader does. It is
  missing off the client, refused only while another mod holds the merge,
  accepted again for the mod that holds it, and too late after init (#1065).
