# 🏛️ DevTools Mastery Handbook: The Invariant Architecture

> **"DevTools are a specialized set of observability and manipulation interfaces
> that run alongside an application, connecting to its runtime via a defined protocol."**

These 6 invariants hold true across Chrome DevTools, Firefox Developer Tools,
React DevTools, Redux DevTools, and native debuggers like LLDB/GDB.

---

## The Architecture (Client-Server / Protocol-Driven)

Unlike compilers (pipelines) or IDEs (reactive graphs), DevTools are fundamentally **client-server systems**, even when running on the same machine.

```text
    [Target Application / Runtime]
                 ↕
INV-01: Debug Protocol (e.g., CDP, DAP)
                 ↕
    [DevTools Frontend / Client]
                 ↓
INV-02: Source Mapping (compiled → source)
                 ↓
INV-03: State Inspection & Mutation (DOM, Scope, Memory)
                 ↓
INV-04: Network Interception & Mocking
                 ↓
INV-05: Profiling & Tracing (CPU, Memory, Paint)
                 ↓
INV-06: Console & REPL (Evaluation in context)
```

---

## 🧱 The 6 Universal Invariants

### INV-01: Debug Protocol (The Bridge)
> The standardized language used to communicate between the tool and the runtime.
- Chrome DevTools Protocol (CDP), Debug Adapter Protocol (DAP), Java Debug Wire Protocol (JDWP)
- Bidirectional: Commands (Tool → Runtime), Events (Runtime → Tool)
- **Critical insight**: DevTools are just UI clients for these protocols.

### INV-02: Source Mapping
> The ability to translate executed code back to authored code.
- Source Maps (Web), DWARF (Native)
- Translates execution coordinates (binary offset, compiled line/col) to source coordinates.
- Without this, debugging minified or compiled code is impossible.

### INV-03: State Inspection & Mutation
> Pausing execution and examining/changing the internal state.
- Reading variables in scope, evaluating expressions in a specific frame.
- DOM inspection (Elements panel), React component trees.
- Memory inspection (Heaps, Registers).

### INV-04: Network Interception
> Monitoring and modifying I/O operations.
- Request/Response logging.
- Throttling (simulating slow networks).
- Request blocking and response mocking.

### INV-05: Profiling & Tracing
> Collecting performance data over time for analysis.
- CPU profiling (sampling vs. instrumentation).
- Memory allocation tracking and heap snapshots.
- Flame graphs and timeline visualization.

### INV-06: Console & REPL
> Interactive execution of code within the context of the running application.
- Execution context isolation (evaluating in a specific frame or global scope).
- Object formatting and expandable previews.
- Autocomplete based on runtime state.

---

## Upstream References
- `references/devtools/chrome-devtools-frontend` — The official frontend for Chrome DevTools

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Debug Protocol | `[ ]` Not started |
| 02 | Source Mapping | `[ ]` Not started |
| 03 | State Inspection | `[ ]` Not started |
| 04 | Network Interception | `[ ]` Not started |
| 05 | Profiling | `[ ]` Not started |
| 06 | Console REPL | `[ ]` Not started |
