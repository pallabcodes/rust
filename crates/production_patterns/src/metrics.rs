//! Observability and Metrics for Production Systems
//!
//! Comprehensive observability is crucial for production systems. This module
//! provides structured metrics collection, tracing integration, and monitoring
//! patterns that enable effective system observability.
//!
//! ## Key Concepts
//!
//! - **Metrics**: Counters, gauges, histograms for quantitative monitoring
//! - **Tracing**: Request tracing and distributed tracing support
//! - **Health Checks**: System health assessment
//! - **Dashboards**: Metrics aggregation and visualization
//! - **Alerts**: Threshold-based alerting on metrics
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::metrics::{MetricsCollector, Counter, Gauge};
//!
//! let metrics = MetricsCollector::new();
//!
//! // Create counters and gauges
//! let requests = metrics.counter("http_requests_total");
//! let active_connections = metrics.gauge("active_connections");
//!
//! // Record metrics
//! requests.increment();
//! active_connections.set(42);
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, instrument};

/// Central metrics collector
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    counters: Arc<RwLock<HashMap<String, Counter>>>,
    gauges: Arc<RwLock<HashMap<String, Gauge>>>,
    histograms: Arc<RwLock<HashMap<String, Histogram>>>,
    namespace: Option<String>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            namespace: None,
        }
    }

    /// Create a namespaced collector
    pub fn with_namespace(namespace: impl Into<String>) -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            namespace: Some(namespace.into()),
        }
    }

    /// Get or create a counter
    pub async fn counter(&self, name: &str) -> CounterHandle {
        let full_name = self.full_name(name);
        let mut counters = self.counters.write().await;

        if !counters.contains_key(&full_name) {
            counters.insert(full_name.clone(), Counter::new());
        }

        CounterHandle {
            name: full_name,
            collector: self.clone(),
        }
    }

    /// Get or create a gauge
    pub async fn gauge(&self, name: &str) -> GaugeHandle {
        let full_name = self.full_name(name);
        let mut gauges = self.gauges.write().await;

        if !gauges.contains_key(&full_name) {
            gauges.insert(full_name.clone(), Gauge::new());
        }

        GaugeHandle {
            name: full_name,
            collector: self.clone(),
        }
    }

    /// Get or create a histogram
    pub async fn histogram(&self, name: &str, buckets: Vec<f64>) -> HistogramHandle {
        let full_name = self.full_name(name);
        let mut histograms = self.histograms.write().await;

        if !histograms.contains_key(&full_name) {
            histograms.insert(full_name.clone(), Histogram::new(buckets));
        }

        HistogramHandle {
            name: full_name,
            collector: self.clone(),
        }
    }

    /// Export all metrics in Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // Export counters
        let counters = self.counters.read().await;
        for (name, counter) in counters.iter() {
            output.push_str(&format!("# HELP {} Counter metric\n", name));
            output.push_str(&format!("# TYPE {} counter\n", name));
            output.push_str(&format!("{} {}\n", name, counter.get()));
        }

        // Export gauges
        let gauges = self.gauges.read().await;
        for (name, gauge) in gauges.iter() {
            output.push_str(&format!("# HELP {} Gauge metric\n", name));
            output.push_str(&format!("# TYPE {} gauge\n", name));
            output.push_str(&format!("{} {}\n", name, gauge.get()));
        }

        // Export histograms
        let histograms = self.histograms.read().await;
        for (name, histogram) in histograms.iter() {
            output.push_str(&format!("# HELP {} Histogram metric\n", name));
            output.push_str(&format!("# TYPE {} histogram\n", name));

            let stats = histogram.stats();
            output.push_str(&format!("{}_count {}\n", name, stats.count));
            output.push_str(&format!("{}_sum {}\n", name, stats.sum));

            for (le, count) in stats.buckets {
                output.push_str(&format!("{}_bucket{{le=\"{}\"}} {}\n", name, le, count));
            }
        }

        output
    }

    /// Get snapshot of all metrics
    pub async fn snapshot(&self) -> MetricsSnapshot {
        let counters = self.counters.read().await
            .iter()
            .map(|(k, v)| (k.clone(), v.get()))
            .collect();

        let gauges = self.gauges.read().await
            .iter()
            .map(|(k, v)| (k.clone(), v.get()))
            .collect();

        let histograms = self.histograms.read().await
            .iter()
            .map(|(k, v)| (k.clone(), v.stats()))
            .collect();

        MetricsSnapshot {
            counters,
            gauges,
            histograms,
            timestamp: Instant::now(),
        }
    }

    fn full_name(&self, name: &str) -> String {
        match &self.namespace {
            Some(ns) => format!("{}_{}", ns, name),
            None => name.to_string(),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Counter metric (monotonically increasing)
#[derive(Debug)]
pub struct Counter {
    value: std::sync::atomic::AtomicU64,
}

impl Counter {
    fn new() -> Self {
        Self {
            value: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn increment(&self) {
        self.value.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn add(&self, delta: u64) {
        self.value.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Gauge metric (can go up and down)
#[derive(Debug)]
pub struct Gauge {
    value: std::sync::atomic::AtomicI64,
}

impl Gauge {
    fn new() -> Self {
        Self {
            value: std::sync::atomic::AtomicI64::new(0),
        }
    }

    pub fn set(&self, value: i64) {
        self.value.store(value, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn increment(&self) {
        self.value.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn decrement(&self) {
        self.value.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn add(&self, delta: i64) {
        self.value.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> i64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Histogram metric (distribution of values)
#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<f64>,
    counts: Vec<std::sync::atomic::AtomicU64>,
    sum: std::sync::atomic::AtomicU64,
    count: std::sync::atomic::AtomicU64,
}

impl Histogram {
    fn new(buckets: Vec<f64>) -> Self {
        let mut counts = Vec::with_capacity(buckets.len() + 1);
        for _ in 0..=buckets.len() {
            counts.push(std::sync::atomic::AtomicU64::new(0));
        }

        Self {
            buckets,
            counts,
            sum: std::sync::atomic::AtomicU64::new(0),
            count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn observe(&self, value: f64) {
        let value_u64 = (value * 1_000_000.0) as u64; // Convert to micro-units for integer arithmetic

        self.sum.fetch_add(value_u64, std::sync::atomic::Ordering::Relaxed);
        self.count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Find the appropriate bucket
        let mut bucket_index = 0;
        for (i, &bucket) in self.buckets.iter().enumerate() {
            if value <= bucket {
                bucket_index = i;
                break;
            }
        }

        // If value is larger than all buckets, it goes in the last bucket
        if value > *self.buckets.last().unwrap_or(&f64::INFINITY) {
            bucket_index = self.buckets.len();
        }

        self.counts[bucket_index].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn stats(&self) -> HistogramStats {
        let sum = self.sum.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1_000_000.0;
        let count = self.count.load(std::sync::atomic::Ordering::Relaxed);

        let mut buckets = Vec::new();
        for (i, count_atomic) in self.counts.iter().enumerate() {
            let le = if i < self.buckets.len() {
                self.buckets[i]
            } else {
                f64::INFINITY
            };
            let count = count_atomic.load(std::sync::atomic::Ordering::Relaxed);
            buckets.push((le, count));
        }

        HistogramStats {
            count,
            sum,
            buckets,
        }
    }
}

/// Handle for counter operations
#[derive(Debug)]
pub struct CounterHandle {
    name: String,
    collector: MetricsCollector,
}

impl CounterHandle {
    pub async fn increment(&self) {
        let counters = self.collector.counters.read().await;
        if let Some(counter) = counters.get(&self.name) {
            counter.increment();
            debug!("Incremented counter: {}", self.name);
        }
    }

    pub async fn add(&self, delta: u64) {
        let counters = self.collector.counters.read().await;
        if let Some(counter) = counters.get(&self.name) {
            counter.add(delta);
            debug!("Added {} to counter: {}", delta, self.name);
        }
    }

    pub async fn get(&self) -> u64 {
        let counters = self.collector.counters.read().await;
        counters.get(&self.name).map(|c| c.get()).unwrap_or(0)
    }
}

/// Handle for gauge operations
#[derive(Debug)]
pub struct GaugeHandle {
    name: String,
    collector: MetricsCollector,
}

impl GaugeHandle {
    pub async fn set(&self, value: i64) {
        let gauges = self.collector.gauges.read().await;
        if let Some(gauge) = gauges.get(&self.name) {
            gauge.set(value);
            debug!("Set gauge {} to: {}", self.name, value);
        }
    }

    pub async fn increment(&self) {
        let gauges = self.collector.gauges.read().await;
        if let Some(gauge) = gauges.get(&self.name) {
            gauge.increment();
            debug!("Incremented gauge: {}", self.name);
        }
    }

    pub async fn decrement(&self) {
        let gauges = self.collector.gauges.read().await;
        if let Some(gauge) = gauges.get(&self.name) {
            gauge.decrement();
            debug!("Decremented gauge: {}", self.name);
        }
    }

    pub async fn add(&self, delta: i64) {
        let gauges = self.collector.gauges.read().await;
        if let Some(gauge) = gauges.get(&self.name) {
            gauge.add(delta);
            debug!("Added {} to gauge: {}", delta, self.name);
        }
    }

    pub async fn get(&self) -> i64 {
        let gauges = self.collector.gauges.read().await;
        gauges.get(&self.name).map(|g| g.get()).unwrap_or(0)
    }
}

/// Handle for histogram operations
#[derive(Debug)]
pub struct HistogramHandle {
    name: String,
    collector: MetricsCollector,
}

impl HistogramHandle {
    pub async fn observe(&self, value: f64) {
        let histograms = self.collector.histograms.read().await;
        if let Some(histogram) = histograms.get(&self.name) {
            histogram.observe(value);
            debug!("Observed value {} in histogram: {}", value, self.name);
        }
    }

    pub async fn stats(&self) -> HistogramStats {
        let histograms = self.collector.histograms.read().await;
        histograms.get(&self.name)
            .map(|h| h.stats())
            .unwrap_or_default()
    }
}

/// Statistics for histogram
#[derive(Debug, Clone)]
pub struct HistogramStats {
    pub count: u64,
    pub sum: f64,
    pub buckets: Vec<(f64, u64)>,
}

impl Default for HistogramStats {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            buckets: Vec::new(),
        }
    }
}

/// Snapshot of all metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, i64>,
    pub histograms: HashMap<String, HistogramStats>,
    pub timestamp: Instant,
}

/// Health check system
#[derive(Debug)]
pub struct HealthChecker {
    checks: Arc<RwLock<Vec<HealthCheck>>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            checks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn register<F>(&self, name: String, check: F)
    where
        F: Fn() -> HealthCheckResult + Send + Sync + 'static,
    {
        let mut checks = self.checks.write().await;
        checks.push(HealthCheck {
            name,
            check: Box::new(check),
        });
    }

    pub async fn check_all(&self) -> HealthStatus {
        let checks = self.checks.read().await;
        let mut results = Vec::new();
        let mut overall_healthy = true;

        for check in checks.iter() {
            let result = (check.check)();
            overall_healthy &= result.healthy;
            results.push((check.name.clone(), result));
        }

        HealthStatus {
            healthy: overall_healthy,
            checks: results,
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct HealthCheck {
    name: String,
    check: Box<dyn Fn() -> HealthCheckResult + Send + Sync>,
}

#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub message: Option<String>,
    pub details: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: Vec<(String, HealthCheckResult)>,
}

/// Alert manager for threshold-based alerting
#[derive(Debug)]
pub struct AlertManager {
    alerts: Arc<RwLock<Vec<AlertRule>>>,
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            alerts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn register_rule(&self, rule: AlertRule) {
        let mut alerts = self.alerts.write().await;
        alerts.push(rule);
    }

    pub async fn evaluate(&self, snapshot: &MetricsSnapshot) -> Vec<Alert> {
        let alerts = self.alerts.read().await;
        let mut fired_alerts = Vec::new();

        for rule in alerts.iter() {
            if let Some(alert) = rule.evaluate(snapshot) {
                fired_alerts.push(alert);
            }
        }

        fired_alerts
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct AlertRule {
    pub name: String,
    pub condition: Box<dyn Fn(&MetricsSnapshot) -> Option<Alert> + Send + Sync>,
}

impl AlertRule {
    pub fn new<F>(name: String, condition: F) -> Self
    where
        F: Fn(&MetricsSnapshot) -> Option<Alert> + Send + Sync + 'static,
    {
        Self {
            name,
            condition: Box::new(condition),
        }
    }

    fn evaluate(&self, snapshot: &MetricsSnapshot) -> Option<Alert> {
        (self.condition)(snapshot)
    }
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counter() {
        let collector = MetricsCollector::new();
        let counter = collector.counter("test_counter").await;

        counter.increment().await;
        counter.add(5).await;

        assert_eq!(counter.get().await, 6);
    }

    #[tokio::test]
    async fn test_gauge() {
        let collector = MetricsCollector::new();
        let gauge = collector.gauge("test_gauge").await;

        gauge.set(10).await;
        gauge.increment().await;
        gauge.add(5).await;
        gauge.decrement().await;

        assert_eq!(gauge.get().await, 15);
    }

    #[tokio::test]
    async fn test_histogram() {
        let collector = MetricsCollector::new();
        let histogram = collector.histogram("test_histogram", vec![1.0, 5.0, 10.0]).await;

        histogram.observe(0.5).await;  // bucket 0
        histogram.observe(3.0).await;  // bucket 1
        histogram.observe(7.0).await;  // bucket 2
        histogram.observe(15.0).await; // bucket 3 (overflow)

        let stats = histogram.stats().await;
        assert_eq!(stats.count, 4);
        assert_eq!(stats.sum, 0.5 + 3.0 + 7.0 + 15.0);
        assert_eq!(stats.buckets.len(), 4); // +inf bucket
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let collector = MetricsCollector::new();

        let counter = collector.counter("requests_total").await;
        counter.add(42).await;

        let gauge = collector.gauge("active_connections").await;
        gauge.set(5).await;

        let output = collector.export_prometheus().await;

        assert!(output.contains("requests_total 42"));
        assert!(output.contains("active_connections 5"));
        assert!(output.contains("# TYPE requests_total counter"));
        assert!(output.contains("# TYPE active_connections gauge"));
    }

    #[tokio::test]
    async fn test_health_checker() {
        let checker = HealthChecker::new();

        checker.register("database".to_string(), || HealthCheckResult {
            healthy: true,
            message: Some("Connected".to_string()),
            details: None,
        }).await;

        checker.register("cache".to_string(), || HealthCheckResult {
            healthy: false,
            message: Some("Connection failed".to_string()),
            details: Some([("error".to_string(), "timeout".to_string())].into_iter().collect()),
        }).await;

        let status = checker.check_all().await;

        assert!(!status.healthy);
        assert_eq!(status.checks.len(), 2);

        let db_check = status.checks.iter().find(|(name, _)| name == "database").unwrap();
        assert!(db_check.1.healthy);

        let cache_check = status.checks.iter().find(|(name, _)| name == "cache").unwrap();
        assert!(!cache_check.1.healthy);
    }

    #[tokio::test]
    async fn test_alert_manager() {
        let manager = AlertManager::new();

        // Alert when counter exceeds 10
        manager.register_rule(AlertRule::new("high_counter".to_string(), |snapshot| {
            if let Some(&count) = snapshot.counters.get("test_counter") {
                if count > 10 {
                    return Some(Alert {
                        rule_name: "high_counter".to_string(),
                        severity: AlertSeverity::Warning,
                        message: format!("Counter is too high: {}", count),
                        timestamp: Instant::now(),
                    });
                }
            }
            None
        })).await;

        let collector = MetricsCollector::new();
        let counter = collector.counter("test_counter").await;

        // No alert initially
        let snapshot = collector.snapshot().await;
        let alerts = manager.evaluate(&snapshot).await;
        assert!(alerts.is_empty());

        // Add enough to trigger alert
        counter.add(15).await;
        let snapshot = collector.snapshot().await;
        let alerts = manager.evaluate(&snapshot).await;

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].rule_name, "high_counter");
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
    }

    #[tokio::test]
    async fn test_namespaced_collector() {
        let collector = MetricsCollector::with_namespace("myapp");

        let counter = collector.counter("requests").await;
        counter.increment().await;

        let snapshot = collector.snapshot().await;
        assert!(snapshot.counters.contains_key("myapp_requests"));
    }
}
