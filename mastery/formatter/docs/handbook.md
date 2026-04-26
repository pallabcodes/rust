# 🏛️ Formatter Mastery Handbook: The Invariant Architecture

> **"A formatter is a compiler that transforms source code into semantically
> identical but stylistically normalized output, preserving comments and
> non-code elements."**

These 5 invariants hold true across Prettier, rustfmt, gofmt, Black (Python),
clang-format, and every serious code formatter.

> [!IMPORTANT]
> Formatters are NOT just "pretty-printers." They must preserve semantics
> while transforming *only* whitespace and layout. This constraint makes
> them fundamentally different from compilers.

---

## The Pipeline

```text
Source Text
    ↓
INV-01: CST Parser (Concrete Syntax Tree — preserves ALL tokens)
    ↓
INV-02: Formatting Rules / Configuration
    ↓
INV-03: Layout Algorithm (CST → Intermediate Document Model)
    ↓
INV-04: Line Breaking & Indentation Engine
    ↓
INV-05: Output Generation (preserving comments, pragmas, whitespace-sensitive regions)
```

---

## 🧱 The 5 Universal Invariants

### INV-01: Concrete Syntax Tree (CST) Parser
> Source code is parsed into a tree that preserves EVERY token, including whitespace and comments.
- Unlike compilers (which discard whitespace), formatters MUST retain it
- CST vs AST: CST = lossless, AST = lossy
- **Critical insight**: If you lose a comment during parsing, the formatter is broken

### INV-02: Formatting Rules / Configuration
> A declarative set of rules defines the desired output style.
- Tab width, max line length, brace style, trailing commas
- Per-language, per-project, overridable
- **Solutions**: .prettierrc, rustfmt.toml, .editorconfig

### INV-03: Layout Algorithm (Document IR)
> The CST is transformed into an intermediate "Document" model that describes layout intent.
- Wadler-Lindig algorithm (Prettier uses this)
- Document primitives: `group`, `indent`, `line`, `softline`, `hardline`
- **Critical insight**: This is the formatter's "IR" — it decouples parsing from output

### INV-04: Line Breaking & Indentation Engine
> The Document IR is resolved into concrete line breaks and indentation.
- "Does this group fit on one line?" → Yes: flat mode, No: break mode
- Respects max line width constraints
- Handles nested indentation correctly

### INV-05: Output Generation
> The resolved layout is serialized back to source text.
- Must preserve: comments, pragmas, string literals, disable-directives (`// prettier-ignore`)
- Must handle: different line endings (CRLF vs LF), encoding (UTF-8)
- **Idempotency**: formatting already-formatted code must produce identical output

---

## Upstream References
- `references/formatter/prettier` — The reference implementation for Wadler-Lindig in JS
- `references/formatter/rustfmt` — Rust's official formatter

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | CST Parser | `[ ]` Not started |
| 02 | Formatting Rules | `[ ]` Not started |
| 03 | Layout Algorithm | `[ ]` Not started |
| 04 | Line Breaking | `[ ]` Not started |
| 05 | Output Generation | `[ ]` Not started |
