# 🏛️ IDE Mastery Handbook: The Invariant Architecture

> **"A modern IDE is a reactive system that maintains multiple synchronized views
> of code (text, syntax, symbols, UI), driven by an event stream and
> orchestrated asynchronously."**

This handbook defines the **12 Universal Invariants** of a production-grade IDE.
These hold true across VS Code, Zed, IntelliJ IDEA, Xcode, and any serious IDE
— they differ only in *implementation choices*, never in *structural necessity*.

> [!IMPORTANT]
> **Invariants describe the PROBLEM, not the solution.**
> "Piece Table" is NOT an invariant — "Efficient Mutable Text Structure" IS.
> "Tree-sitter" is NOT an invariant — "Incremental Structural Model" IS.
> "Electron" is NOT an invariant — "Rendering Pipeline" IS.

---

## The Reactive Graph (NOT a linear pipeline)

IDEs are not compilers. Compilers are linear pipelines (`source → AST → IR → codegen`).
IDEs are **reactive graphs** where every edit fans out to multiple consumers:

```text
        ┌──────────┐
        │  Buffer  │ ← Invariant 01
        └────┬─────┘
             ↓
    ┌─────────────────┐
    │  Change Stream   │ ← Invariant 02
    └──┬────┬────┬────┘
       ↓    ↓    ↓
   Parser Index  Event System ← Invariants 03, 04, 05
       ↓    ↓       ↓
       └──→ Orchestrator ←──┘ ← Invariant 06
                ↓
         View Model + Virtualization ← Invariants 07, 08
                ↓
         Rendering Pipeline ← Invariant 09
                ↓
         Consistency Model ← Invariant 10 (cross-cutting)
                ↓
         Persistence ← Invariant 11
                ↓
         Extensibility ← Invariant 12
```

---

## 🧱 The 12 Universal Invariants

### INV-01: Editable Text Model (Buffer)
> There is always a structure that represents editable text.
- Supports insert / delete at arbitrary positions
- Tracks cursors & selections
- Supports undo / redo
- **Solutions (NOT invariants)**: Piece Table (VS Code), Rope/B-Tree (Zed), Gap Buffer (simpler editors)

### INV-02: Change Propagation System (Change Stream)
> Every edit produces a stream of changes that other subsystems react to.
```text
edit → diff/change → broadcast → [parser, UI, index, LSP, AI]
```
- This is the hidden backbone — without it, nothing stays in sync.

### INV-03: Incremental Structural Model (Syntax / AST)
> The editor maintains a continuously updated structural understanding of code.
- Syntax tree updated incrementally as the user types (not full reparse)
- Equivalent to a compiler's parse stage, but *continuous*

### INV-04: Global Knowledge Model (Project Index)
> The IDE maintains cross-file understanding.
- Symbols (functions, classes, types)
- References and call graphs
- Dependencies
- Enables: go-to-definition, rename, search, AI context

### INV-05: Event / Command System (Dispatcher)
> All user and system interactions are translated into commands that mutate state.
```text
keypress → command → state change → events
```
- User input, system triggers, plugin actions all flow through this single control plane

### INV-06: Asynchronous Orchestration Layer
> External or heavy work runs outside the main thread.
- Language servers (LSP)
- AI models
- Background indexing
- **Constraints**: non-blocking, cancelable, streaming-friendly

### INV-07: View Model (Projection of State)
> Internal state is transformed into a renderable representation.
- Lines to display, styling (syntax highlighting), cursor positions
- Decouples logic from rendering

### INV-08: Viewport Virtualization
> Only a small visible portion of the code is processed/rendered.
- Visible lines only; scrolling window over large buffer
- Required for performance at scale (million-line files)

### INV-09: Rendering Pipeline
> The view model is converted into pixels.
```text
state → layout → glyphs (text shaping) → GPU → screen
```
- **Solutions (NOT invariants)**: CoreText (macOS), HarfBuzz+FreeType (Linux), Skia (Chromium/VS Code)

### INV-10: Consistency / Versioning Model _(CRITICAL, often ignored)_
> All subsystems operate on coherent snapshots of state.
- Prevents: race conditions, stale AST, delayed AI responses, out-of-order updates
- **Solutions**: Version IDs, snapshots, transactional updates

### INV-11: Persistence Layer
> State can be saved and restored.
- File system integration, workspace state, settings, session recovery

### INV-12: Extensibility Mechanism (Plugins / Integrations)
> The system exposes hooks for extension.
- Commands, events, APIs
- Every major IDE has this (even if limited)

---

## 🗂️ Directory Structure (Invariant-Driven)

Each invariant maps to a numbered directory under `invariant-core/`.
This enforces **sequential understanding** — you cannot implement INV-03 (Parser)
without first understanding INV-01 (Buffer) and INV-02 (Change Stream).

```text
mastery/ide/
├── docs/
│   ├── handbook.md          ← This file (invariant definitions)
│   ├── logs.md              ← Raw research dialogue
│   └── learning_units.md    ← Testable tasks per invariant
│
├── invariant-core/          ← One crate per invariant
│   ├── inv01-buffer/        ← Piece Table / Rope
│   ├── inv02-change-stream/ ← Event broadcasting
│   ├── inv03-parser/        ← Incremental AST
│   ├── inv04-index/         ← Symbol table / project graph
│   ├── inv05-commands/      ← Event / command dispatcher
│   ├── inv06-orchestrator/  ← Async task management
│   ├── inv07-view-model/    ← State → renderable projection
│   ├── inv08-virtualization/ ← Viewport windowing
│   ├── inv09-renderer/      ← GPU text pipeline
│   ├── inv10-consistency/   ← Snapshot / versioning
│   ├── inv11-persistence/   ← File I/O + workspace state
│   └── inv12-extensions/    ← Plugin API surface
│
├── mapping/                 ← Stage 3: How Zed/VS Code implements each invariant
│   ├── inv01-buffer.md      ← "Zed uses Rope in crates/text/"
│   ├── inv02-change-stream.md
│   └── ...
│
└── experiments/             ← Stage 4: Dimensional scaling attempts
```

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Buffer | `[x]` Complete |
| 02 | Change Stream | `[ ]` Not started |
| 03 | Incremental Parser | `[ ]` Not started |
| 04 | Global Index | `[ ]` Not started |
| 05 | Command System | `[ ]` Not started |
| 06 | Async Orchestrator | `[ ]` Not started |
| 07 | View Model | `[ ]` Not started |
| 08 | Viewport Virtualization | `[ ]` Not started |
| 09 | Rendering Pipeline | `[ ]` Not started |
| 10 | Consistency Model | `[ ]` Not started |
| 11 | Persistence | `[ ]` Not started |
| 12 | Extensibility | `[ ]` Not started |
