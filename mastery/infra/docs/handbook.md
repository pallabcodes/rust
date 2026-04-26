# 🏛️ Infrastructure Mastery Handbook: The Invariant Architecture

> **"Infrastructure platforms (like Kubernetes or Nomad) are distributed control planes
> that manage the lifecycle, placement, and networking of workloads across a cluster of machines."**

These 6 invariants hold true across Kubernetes, Nomad, Mesos, Docker Swarm, and
cloud-provider specific schedulers like ECS.

---

## The Control Plane Architecture

Infrastructure platforms are distributed systems composed of agents (nodes) and a centralized/consensus-based brain (control plane).

```text
INV-01: Container Runtime (The Executor)
    ↓
INV-02: Networking / Service Discovery
    ↓
INV-03: Scheduling & Resource Allocation
    ↓
INV-04: Health Checking & Self-Healing
    ↓
INV-05: Configuration & Secrets Management
    ↓
INV-06: Observability (Logs, Metrics, Traces)
```

---

## 🧱 The 6 Universal Invariants

### INV-01: Container Runtime / Execution Engine
> The process that actually runs the workload on a specific machine.
- containerd, CRI-O, Docker, Firecracker (MicroVMs)
- Interfaces with the OS kernel (namespaces, cgroups in Linux) to isolate processes.
- The platform agent (e.g., Kubelet) talks to this runtime.

### INV-02: Networking & Service Discovery
> How workloads communicate with each other and the outside world.
- IP allocation per workload (CNI in Kubernetes).
- DNS-based service discovery (CoreDNS).
- Load balancing and ingress routing.
- This is often the most complex and fragile part of the platform.

### INV-03: Scheduling & Resource Allocation
> Deciding *where* a workload should run based on constraints and available resources.
- Bin-packing algorithms.
- Node affinity, anti-affinity, taints, and tolerations.
- Tracking CPU, Memory, and GPU requests/limits.

### INV-04: Health Checking & Self-Healing
> Continuous monitoring of workload health and automated recovery.
- Liveness and Readiness probes.
- Restarting failed containers.
- Rescheduling workloads if a node dies.
- This is the "control loop" reacting to runtime failures.

### INV-05: Configuration & Secrets Management
> Injecting environment-specific data securely into workloads.
- Decoupling configuration from container images.
- ConfigMaps and Secrets (Kubernetes).
- Secure distribution (encrypted in transit and at rest in the state store).

### INV-06: Observability
> Emitting signals about the platform and workload behavior.
- Log aggregation (stdout/stderr capture).
- Metrics exposing (Prometheus endpoints).
- Distributed tracing injection.

---

## Upstream References
- `references/infra/containerd` — An industry-standard container runtime.
- `references/infra/etcd` — The distributed key-value store (consensus engine) used by Kubernetes.

---

## 🧭 Mastery Progress

| # | Invariant | Status |
|---|-----------|--------|
| 01 | Container Runtime | `[ ]` Not started |
| 02 | Networking | `[ ]` Not started |
| 03 | Scheduling | `[ ]` Not started |
| 04 | Health Checking | `[ ]` Not started |
| 05 | Config & Secrets | `[ ]` Not started |
| 06 | Observability | `[ ]` Not started |
