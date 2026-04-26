# 🏛️ Database Mastery Handbook: The Invariant Architecture

> **"A database management system (DBMS) is an engine that guarantees safe,
> concurrent access to data while providing an abstraction over physical storage."**

These 8 invariants hold true across PostgreSQL, MySQL, SQLite, Oracle, and
most relational/analytical database engines.

---

## The Engine Architecture

Databases are separated into a "Frontend" (parsing and planning) and a "Backend" (execution and storage).

```text
[Frontend / Query Engine]
    ↓
INV-01: Query Parser (SQL → AST)
    ↓
INV-02: Query Planner / Optimizer (AST → Execution Plan)
    ↓
[Backend / Storage Engine]
    ↓
INV-03: Execution Engine (Iterators / Vectorized processing)
    ↓
INV-04: Storage Engine (Disk / Memory Layout, B-Trees, LSM Trees)
    ↓
INV-05: Buffer Pool / Page Manager (Memory Caching)
    ↓
INV-06: Write-Ahead Log (WAL) (Durability)
    ↓
INV-07: Concurrency Control (MVCC / Locks) (Isolation)
    ↓
INV-08: Recovery System (Crash Resilience)
```

---

## 🧱 The 8 Universal Invariants

### INV-01: Query Parser
> Converts the text query (e.g., SQL) into an Abstract Syntax Tree (AST).
- Lexing and parsing (similar to a compiler's frontend).
- Syntax validation.

### INV-02: Query Planner / Optimizer
> Transforms the AST into an optimal execution plan.
- **Logical Optimization**: Applying algebraic rules (e.g., pushing down filters).
- **Physical Optimization**: Choosing the best algorithms (e.g., Hash Join vs. Merge Join) based on cost models and statistics.
- The plan is usually represented as a tree of relational operators.

### INV-03: Execution Engine
> Executes the physical plan to produce results.
- **Volcano Model (Tuple-at-a-time)**: Standard in traditional OLTP (PostgreSQL, MySQL).
- **Vectorized Model (Batch-at-a-time)**: Standard in OLAP/Analytical engines (DuckDB, ClickHouse) for better CPU cache locality.

### INV-04: Storage Engine
> How data is physically organized on disk (or in memory).
- **Row-oriented**: Good for transactional point-queries (PostgreSQL).
- **Column-oriented**: Good for analytical aggregations (DuckDB).
- Indexing structures: B+Trees, LSM Trees, Hash Indexes.
- Data is usually divided into fixed-size "Pages" or "Blocks".

### INV-05: Buffer Pool / Page Manager
> The memory cache that sits between the execution engine and disk.
- Databases do not rely on the OS page cache entirely; they manage their own memory.
- Eviction policies (LRU, Clock sweep) to decide which pages to kick out when memory is full.

### INV-06: Write-Ahead Log (WAL)
> Ensures data durability (the 'D' in ACID).
- Every modification is written sequentially to the WAL *before* the data page is modified in the buffer pool.
- Sequential writes are fast; random writes to data pages happen asynchronously later.

### INV-07: Concurrency Control
> Ensures Isolation (the 'I' in ACID) when multiple transactions access data simultaneously.
- **Pessimistic**: Locking (Two-Phase Locking - 2PL).
- **Optimistic**: Multi-Version Concurrency Control (MVCC) — Readers don't block writers, writers don't block readers.

### INV-08: Recovery System
> Restores the database to a consistent state after a crash.
- Uses the WAL to replay committed transactions that weren't written to disk (Redo).
- Uses the WAL to undo uncommitted transactions that were partially written to disk (Undo).
- ARIES algorithm is the classic reference.

---

## Upstream References
- `references/database/duckdb` — An incredible example of a modern vectorized, analytical engine.
- `references/database/tinysql` — A minimal implementation of a SQL engine.

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Query Parser | `[ ]` Not started |
| 02 | Query Planner | `[ ]` Not started |
| 03 | Execution Engine | `[ ]` Not started |
| 04 | Storage Engine | `[ ]` Not started |
| 05 | Buffer Pool | `[ ]` Not started |
| 06 | WAL | `[ ]` Not started |
| 07 | Concurrency Control | `[ ]` Not started |
| 08 | Recovery | `[ ]` Not started |
