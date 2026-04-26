# 🏛️ Compiler Mastery Handbook: The Invariant Architecture

> **"A compiler is a pipeline that transforms source text into executable machine
> code through a series of increasingly lower-level intermediate representations."**

These 8 invariants hold true across GCC, LLVM/Clang, Cranelift, rustc, Go compiler,
and every serious compiler — they differ only in implementation, never in structure.

> [!IMPORTANT]
> **Invariants describe the PROBLEM, not the solution.**
> "LLVM IR" is NOT an invariant — "Intermediate Representation" IS.
> "Pratt Parser" is NOT an invariant — "Syntactic Analysis" IS.

---

## The Pipeline (Linear, unlike IDEs)

Compilers ARE linear pipelines (unlike IDEs which are reactive graphs):

```text
Source Text
    ↓
INV-01: Lexer (Tokenizer)
    ↓
INV-02: Parser (AST Construction)
    ↓
INV-03: Semantic Analysis (Type Checking, Name Resolution)
    ↓
INV-04: Intermediate Representation (IR)
    ↓
INV-05: Optimization Passes
    ↓
INV-06: Code Generation (Target-Specific)
    ↓
INV-07: Linking & Loading
    ↓
INV-08: Diagnostics & Error Reporting (cross-cutting, touches all stages)
```

---

## 🧱 The 8 Universal Invariants

### INV-01: Lexical Analysis (Tokenizer / Lexer)
> Source text is decomposed into a stream of tokens.
- Whitespace handling, keyword recognition, literal parsing
- **The boundary**: raw bytes → structured token stream
- **Solutions**: Hand-written lexer (rustc, Go), Generated (flex/lex)

### INV-02: Syntactic Analysis (Parser / AST Construction)
> Token stream is organized into a hierarchical Abstract Syntax Tree.
- Grammar rules define legal structure
- Error recovery (partial parsing on invalid input)
- **Solutions**: Recursive descent (most production compilers), PEG, Parser combinators, LR/LALR

### INV-03: Semantic Analysis (Type Checking, Name Resolution)
> The AST is validated for *meaning*, not just *structure*.
- Name resolution (what does `x` refer to?)
- Type checking (is `x + y` valid?)
- Scope analysis, lifetime analysis (Rust borrow checker lives here)
- **This is where most compiler complexity lives**

### INV-04: Intermediate Representation (IR)
> The validated AST is lowered into a simpler, more uniform representation.
- Closer to machine semantics than source semantics
- Enables target-independent optimization
- **Solutions**: SSA form (LLVM IR), CPS, ANF, MIR (rustc), Sea of Nodes (V8 TurboFan)

### INV-05: Optimization Passes
> The IR is transformed to produce equivalent but faster/smaller code.
- Dead code elimination, constant folding, inlining, loop unrolling
- Each pass: IR → IR (composable, ordered)
- **Critical insight**: Optimization is a *series of IR-to-IR transformations*

### INV-06: Code Generation (Target-Specific)
> The optimized IR is translated into target machine instructions.
- Instruction selection, register allocation, instruction scheduling
- Target-specific: x86-64, ARM64, WASM, RISC-V
- **Solutions**: Cranelift (Rust), LLVM backend, direct emission

### INV-07: Linking & Loading
> Separately compiled units are combined into a final executable.
- Symbol resolution across object files
- Relocation, dynamic linking
- **Solutions**: ld (GNU), lld (LLVM), mold (modern fast linker)

### INV-08: Diagnostics & Error Reporting (Cross-Cutting)
> Every stage must produce human-readable, actionable error messages.
- Source location tracking (span information)
- Error recovery (don't stop at first error)
- Warning levels, lint integration
- **This is NOT optional** — production compilers live or die by error quality

---

## 🗂️ Directory Structure

```text
mastery/compiler/
├── docs/
│   ├── handbook.md
│   ├── logs.md
│   └── learning_units.md
├── invariant-core/
│   ├── inv01-lexer/
│   ├── inv02-parser/
│   ├── inv03-semantic-analysis/
│   ├── inv04-intermediate-representation/
│   ├── inv05-optimization/
│   ├── inv06-codegen/
│   ├── inv07-linking/
│   └── inv08-diagnostics/
├── mapping/
└── experiments/
```

## Upstream References
- `references/compiler/the-super-tiny-compiler` — The "Stage 1 Tiny Model" (~200 lines)
- `references/compiler/wasmtime` — Contains Cranelift, a production Rust-native code generator

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Lexer | `[ ]` Not started |
| 02 | Parser | `[ ]` Not started |
| 03 | Semantic Analysis | `[ ]` Not started |
| 04 | IR | `[ ]` Not started |
| 05 | Optimization | `[ ]` Not started |
| 06 | Code Generation | `[ ]` Not started |
| 07 | Linking | `[ ]` Not started |
| 08 | Diagnostics | `[ ]` Not started |
