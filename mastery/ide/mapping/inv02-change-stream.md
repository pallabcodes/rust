# INV-02: Change Stream — Zed Mapping

## Purpose
Map our Change Stream (pub/sub event broadcasting) to Zed's change propagation mechanism.

## Key Questions
- [ ] How does Zed notify subsystems when a buffer changes?
- [ ] Is it channel-based, callback-based, or reactive-stream-based?
- [ ] What data is included in a change notification?
- [ ] How does Zed handle ordering and versioning of changes?

## Zed Source Locations
- TBD (likely in `crates/text/` or `crates/editor/`)

## Comparison Table

| Aspect | Our Implementation | Zed |
|--------|-------------------|-----|
| Mechanism | TBD | TBD |
| Event payload | TBD | TBD |
| Ordering guarantee | TBD | TBD |

## Insights
_(Fill in after tracing)_
