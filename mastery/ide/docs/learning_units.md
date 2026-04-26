# 🎫 Learning Units: AI-Based IDE (Invariant-Sequential)

Each learning unit is tied to a specific invariant. Work them **in order** — 
INV-02 depends on INV-01, INV-03 depends on INV-02, etc.

---

## INV-01: Editable Text Model (Buffer)

- [ ] **LU-01.1**: Implement a Piece Table that supports `insert(offset, text)` and `delete(offset, len)`
    - **DoD**: All operations produce correct output verified against naive `String` concatenation
- [ ] **LU-01.2**: Add `Snapshot` — a thread-safe, immutable clone of buffer state
    - **DoD**: Snapshot taken before edit returns pre-edit content; snapshot taken after returns post-edit
- [ ] **LU-01.3**: Add `LineMap` — translate `(line, col)` ↔ `absolute_offset` in O(log N)
    - **DoD**: Correctly handles multi-byte UTF-8 and CRLF/LF line endings
- [ ] **LU-01.4**: Map to Zed — locate Zed's Rope implementation, document differences vs Piece Table
    - **DoD**: Written comparison in `mapping/inv01-buffer.md`

## INV-02: Change Propagation System

- [ ] **LU-02.1**: Define a `ChangeEvent` struct (offset, old_len, new_len, new_text, version_id)
    - **DoD**: Every `Buffer::insert` and `Buffer::delete` emits a `ChangeEvent`
- [ ] **LU-02.2**: Implement a `ChangeStream` (pub/sub) that fans out events to registered subscribers
    - **DoD**: Multiple subscribers receive the same event; order is preserved
- [ ] **LU-02.3**: Map to Zed — locate how Zed propagates buffer changes to its subsystems
    - **DoD**: Written analysis in `mapping/inv02-change-stream.md`

## INV-03: Incremental Parser

- [ ] **LU-03.1**: Integrate Tree-sitter and parse a buffer snapshot into a syntax tree
    - **DoD**: Syntax tree updates incrementally on change events (not full reparse)
- [ ] **LU-03.2**: Expose syntax node queries (e.g., "find function at line 42")
    - **DoD**: Query returns correct node kind and byte range

## INV-04: Global Index

- [ ] **LU-04.1**: Build a `SymbolTable` that stores (name, kind, file, range) tuples
    - **DoD**: Supports lookup by name and by file
- [ ] **LU-04.2**: Populate the index from parsed ASTs across multiple files
    - **DoD**: "Go-to-definition" resolves correctly across 2+ files

## INV-05: Event / Command System

- [ ] **LU-05.1**: Implement a `CommandDispatcher` that maps keybindings → named commands → state mutations
    - **DoD**: A "type character" command flows through the dispatcher and modifies the buffer

## INV-06: Async Orchestrator

- [ ] **LU-06.1**: Implement a task runner that executes background work on a thread pool
    - **DoD**: Tasks are cancelable; results are delivered via a channel
- [ ] **LU-06.2**: Wire up a mock LSP client that sends/receives JSON-RPC over stdin/stdout
    - **DoD**: Completion request returns a response without blocking the main thread

## INV-07–12: (Defined after INV-01–06 are complete)

These will be defined once the "core reactive loop" (Buffer → Change Stream → Parser → 
Index → Commands → Orchestrator) is operational. Premature definition would be guessing.
