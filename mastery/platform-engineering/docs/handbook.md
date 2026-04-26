# 🏛️ Platform Engineering Mastery Handbook: The Invariant Architecture

> **"Platform engineering tools (Infrastructure as Code) are control systems that
> reconcile a desired declarative state with the actual state of external providers."**

These 6 invariants hold true across Terraform, Pulumi, Crossplane, AWS CloudFormation,
and Kubernetes Operators.

---

## The Reconciliation Loop (Control Theory)

IaC tools are not scripts; they are control loops.

```text
INV-01: Declarative State Definition (The "Desired" State)
    ↓
INV-02: State Reconciliation (Diffing engine)
    ↓
INV-03: Dependency Graph (Execution order)
    ↓
INV-04: Provider Abstraction (The APIs)
    ↓
INV-05: State Storage (The "Actual/Known" State)
    ↓
INV-06: Plan & Apply Engine (Execution phase)
```

---

## 🧱 The 6 Universal Invariants

### INV-01: Declarative State
> The user defines WHAT they want, not HOW to get there.
- HCL (Terraform), YAML (Kubernetes), Code (Pulumi/CDK)
- The tool must parse this into an internal representation of desired resources.

### INV-02: State Reconciliation (Diffing)
> Comparing the "Desired" state against the "Actual" state to compute changes.
- Requires knowing what currently exists (State Storage) and what should exist (Declarative State).
- Produces a set of actions: Create, Read, Update, Delete (CRUD).

### INV-03: Dependency Graph
> Resources depend on each other and must be evaluated/created in the correct order.
- Directed Acyclic Graph (DAG) construction.
- Implicit dependencies (Resource A references Resource B's output).
- Explicit dependencies (`depends_on`).

### INV-04: Provider Abstraction
> A plugin system to interface with external APIs (AWS, GCP, GitHub).
- The core engine doesn't know how to create an S3 bucket.
- Providers expose a standardized RPC interface (e.g., Terraform Provider Protocol via gRPC) to perform the CRUD operations.

### INV-05: State Storage
> The tool must remember what it manages.
- Terraform `.tfstate`, Kubernetes etcd, Pulumi Service.
- Maps the logical resource name in code to the physical resource ID in the cloud.
- Locks are required for concurrent operations.

### INV-06: Plan & Apply Engine
> The execution phase where calculated changes are enacted.
- **Plan**: Dry-run simulation of what will change.
- **Apply**: Executing the plan by walking the DAG and calling Provider APIs.
- Handling partial failures and rollbacks.

---

## Upstream References
- `references/platform-engineering/terraform` — HashiCorp Terraform (Core engine)

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Declarative State | `[ ]` Not started |
| 02 | State Reconciliation | `[ ]` Not started |
| 03 | Dependency Graph | `[ ]` Not started |
| 04 | Provider Abstraction | `[ ]` Not started |
| 05 | State Storage | `[ ]` Not started |
| 06 | Plan & Apply | `[ ]` Not started |
