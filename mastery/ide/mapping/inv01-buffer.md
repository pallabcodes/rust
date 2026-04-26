# INV-01: Buffer — Zed Mapping

## Purpose
Map our Piece Table implementation to Zed's Rope (B-Tree) implementation.

## Key Questions
- [ ] Where is Zed's text buffer defined?
- [ ] What data structure does Zed use (Rope variant)?
- [ ] How does Zed handle undo/redo at the buffer level?
- [ ] How does Zed's `Snapshot` compare to ours?

## Zed Source Locations
- `crates/text/` — likely core text buffer
- `crates/editor/` — editor-level buffer wrapper

## Comparison Table

| Aspect | Our Implementation | Zed |
|--------|-------------------|-----|
| Data structure | Piece Table | TBD |
| Snapshot model | Arc<[Piece]> clone | TBD |
| Line indexing | TBD | TBD |
| Undo/Redo | TBD | TBD |

## Insights
_(Fill in after tracing)_
