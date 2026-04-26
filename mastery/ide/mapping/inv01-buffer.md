# INV-01: Buffer — Zed Mapping

## Purpose
Map our Piece Table implementation to Zed's Rope (B-Tree) implementation.

## Key Questions
- [x] Where is Zed's text buffer defined?
    - `crates/text/src/text.rs`
- [x] What data structure does Zed use (Rope variant)?
    - Zed uses a standard `Rope` for the text itself (`crates/rope`), but wraps it in a `SumTree` (a custom B-Tree variant) of `Fragment`s. This is because Zed's buffer is a CRDT (Conflict-free Replicated Data Type) built for multiplayer editing.
- [x] How does Zed handle undo/redo at the buffer level?
    - Zed uses a `History` struct (defined in `text.rs`) that tracks `Transaction`s. Since the buffer is a CRDT, undos are technically "inverse operations" applied to the CRDT log, tracked via Lamport clocks.
- [x] How does Zed's `Snapshot` compare to ours?
    - Our `Snapshot` is a simple `Arc<[Piece]>`. Zed's `BufferSnapshot` is much richer: it contains `visible_text: Rope`, `deleted_text: Rope`, and `fragments: SumTree<Fragment>`. Since `Rope` and `SumTree` are purely functional (persistent) data structures, snapshotting in Zed is extremely cheap (just atomic reference bumps), similar to our `Arc` cloning, but scales infinitely better.

## Zed Source Locations
- `crates/text/src/text.rs` — The core `Buffer` and `BufferSnapshot` definitions.
- `crates/sum_tree` — The custom B-Tree used to index fragments and edits.
- `crates/rope` — The underlying string data structure.

## Comparison Table

| Aspect | Our Implementation | Zed (`crates/text`) |
|--------|-------------------|----------------------|
| Data structure | Simple Piece Table (`Vec<Piece>`) | `Rope` + `SumTree<Fragment>` (CRDT) |
| Snapshot model | `Arc<[Piece]>` clone | Persistent `Rope` / `SumTree` clone |
| Line indexing | Auxiliary `LineMap` (Binary Search) | Handled internally by the `Rope` tree nodes |
| Undo/Redo | None (omitted for simplicity) | `History` struct with Lamport clock transactions |
| Collaboration | Single-player | Multiplayer-native CRDT |

## Insights
1. **The CRDT Requirement**: Zed's buffer is wildly more complex than a standard Piece Table because it *must* support multiplayer. Every insertion is a `Fragment` tagged with a Lamport timestamp and a Replica ID.
2. **Persistent Data Structures**: Zed relies heavily on "Persistent Data Structures" (like their `SumTree`). When they take a snapshot, they don't lock the buffer; they just take a reference to the root of the tree. When new edits happen, new tree nodes are created, leaving the old snapshot intact. This is the L7 evolution of our `Arc<[Piece]>`.
3. **Dual Ropes**: Zed keeps both `visible_text` and `deleted_text`. When text is deleted, it isn't removed from memory; it's moved to the `deleted_text` Rope. This allows for perfect undo and collaborative conflict resolution.
