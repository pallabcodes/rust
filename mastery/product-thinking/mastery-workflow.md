# 🛠️ Operational Standard: The Mastery Workflow

This document defines the system for organizing learning deep-dives into complex codebases.

---

## 🧬 Core Principle: Invariant-First, Sequential Mastery

Before writing any code, you must:

1. **Extract the Universal Invariants** — the non-negotiable truths of the domain that exist in every production system, regardless of implementation.
2. **Number them sequentially** — dependencies flow downward (INV-02 depends on INV-01).
3. **The folder structure mirrors the invariant sequence** — this enforces discipline and makes progress visible at a glance.

---

## 📁 Repository Structure (Per Domain)

```text
mastery/
└── [domain]/                    (e.g., ide, browser, db, compiler)
    ├── docs/
    │   ├── handbook.md           ← Invariant definitions + reactive architecture
    │   ├── logs.md               ← Raw research dialogue (the "rough")
    │   └── learning_units.md     ← Testable tasks, organized by invariant number
    │
    ├── invariant-core/           ← One directory per invariant (numbered)
    │   ├── inv01-[name]/         ← Implementation of invariant 1
    │   ├── inv02-[name]/         ← Implementation of invariant 2
    │   └── ...
    │
    ├── mapping/                  ← Stage 3: Production repo → invariant alignment
    │   ├── inv01-[name].md       ← "How does [reference] implement INV-01?"
    │   ├── inv02-[name].md
    │   └── ...
    │
    └── experiments/              ← Stage 4: Dimensional scaling (concurrency, perf, etc.)
```

---

## 🔄 The Branching Protocol

### In the Mastery Repo:
- **`main`**: Contains the stable Handbook and verified Invariant Core implementations.
- **`dimension/[name]`**: Branches for heavy Stage 4 experiments (e.g., `dimension/lock-free-buffer`).

### In Reference Production Repos (e.g., Zed, Ladybird):
- **`mastery/trace-[invariant]`**: Branches for adding probes/breakpoints to trace a specific invariant's implementation. **Never merge these.**

---

## 🎯 Workflow Per Invariant

For each `INV-XX`:

1. **Define** the invariant in `docs/handbook.md` (the "What" and "Why")
2. **Implement** a minimal version in `invariant-core/invXX-[name]/`
3. **Map** to a production repo in `mapping/invXX-[name].md` (the "How do they do it?")
4. **Record insights** in `docs/logs.md` (what surprised you? what gap appeared?)
5. **Move to INV-(XX+1)** only after the current invariant is understood and verified
