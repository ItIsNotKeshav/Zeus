# Keeping Zeus's own code apart from upstream's

Upstream ZapFast's text stays where upstream has it, so its changes merge in
place. Code that only Zeus has goes in child modules that upstream does not
have.

- `src/ui/conversation/` holds every top-level item of `ui/conversation.rs`
  that upstream never had. "Never had" means no item of the same kind and name
  anywhere under `src/`, at 0.20.1 (`b0d54b6`) or at `upstream/main`.
  - Each child is named after the part of the view it serves.
  - Each child starts with `use super::*;`, and the parent glob-imports each
    child back, so every name still resolves from the same place.
  - A moved item's only edits were these: private items, fields and methods
    became `pub(super)`, `pub(super)` became `pub(in crate::ui)`, and
    `super::` became `crate::ui::`.
- Upstream functions that Zeus changed stay in `conversation.rs`. Moving
  them would turn each upstream change to them into a manual port.
- Trial merge of `upstream/main`'s `conversation.rs` against base `b0d54b6`
  (`git merge-file`):
  - before the move: 70 conflict hunks, 4,476 conflicted lines;
  - after: 62 hunks, 3,145 lines.

  What still conflicts is Zeus's edits inside upstream's functions.
- New Zeus-only code in this view goes in the matching
  `src/ui/conversation/` file, not in `conversation.rs`.
- When upstream adds an item that Zeus already has under the same name in a
  child module, keep one copy and delete the other.
