# 🏛️ Interpreter Mastery Handbook: The Invariant Architecture

> **"An interpreter is a program that directly executes source code, either by
> walking the AST or by compiling to bytecode and running it on a virtual machine."**

These 7 invariants hold true across CPython, Lua, Ruby (CRuby/YARV), V8 (ignition),
and every serious interpreter.

---

## The Pipeline (Linear with a feedback loop via REPL)

```text
Source Text
    ↓
INV-01: Lexer (Tokenizer)
    ↓
INV-02: Parser (AST Construction)
    ↓
INV-03: Bytecode Compiler (AST → Bytecode)  ← optional for tree-walkers
    ↓
INV-04: Virtual Machine / Execution Engine
    ↓
INV-05: Runtime Environment (GC, Stack, Heap)
    ↓
INV-06: FFI / Host Integration
    ↑
INV-07: REPL (feeds back to INV-01)
```

---

## 🧱 The 7 Universal Invariants

### INV-01: Lexical Analysis (Tokenizer)
> Source text → token stream. Identical to compilers.
- Keywords, identifiers, literals, operators
- Whitespace significance varies (Python vs Lua)

### INV-02: Syntactic Analysis (Parser / AST)
> Tokens → hierarchical tree structure.
- Recursive descent is dominant in interpreters (simpler, faster startup)
- Error recovery matters even more than compilers (REPL usage)

### INV-03: Bytecode Compiler
> AST → compact instruction sequence for a virtual machine.
- Not all interpreters have this (tree-walkers skip it)
- But ALL serious/fast interpreters compile to bytecode (CPython, Lua, V8 Ignition, Ruby YARV)
- **Critical insight**: Bytecode is the interpreter's "IR"

### INV-04: Virtual Machine / Execution Engine
> The bytecode (or AST) is executed instruction by instruction.
- Stack-based VM (CPython, Lua, JVM) vs Register-based VM (LuaJIT, Dalvik)
- Dispatch loop: `while (true) { switch(opcode) { ... } }`
- **This is the interpreter's "CPU"**

### INV-05: Runtime Environment (GC, Stack, Heap)
> Memory and execution state management.
- Garbage Collection (mark-sweep, generational, ref-counting)
- Call stack management
- Object model (how are values represented in memory?)
- **Solutions**: Tagged pointers (Lua), NaN-boxing (SpiderMonkey), PyObject (CPython)

### INV-06: Foreign Function Interface (FFI / Host Integration)
> The interpreter can call into native code and vice versa.
- C API (CPython, Lua), N-API (Node.js)
- Embedding the interpreter in a host application
- **Critical for production use** — no interpreter is an island

### INV-07: REPL / Interactive Evaluation Loop
> Read → Eval → Print → Loop.
- Incremental compilation/evaluation
- State persistence across evaluations
- Error handling without crashing the session

---

## Upstream References
- `references/interpreter/crafting-interpreters` — Bob Nystrom's definitive learning resource
- `references/interpreter/lua` — The gold standard small interpreter (~20K lines, legendary design)

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Lexer | `[ ]` Not started |
| 02 | Parser | `[ ]` Not started |
| 03 | Bytecode Compiler | `[ ]` Not started |
| 04 | Virtual Machine | `[ ]` Not started |
| 05 | Runtime (GC) | `[ ]` Not started |
| 06 | FFI | `[ ]` Not started |
| 07 | REPL | `[ ]` Not started |
