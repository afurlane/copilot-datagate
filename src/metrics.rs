//! Lightweight in-process metrics for MCP operations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolMetrics {
    pub size: u32,
    pub idle: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub requests_total: u64,
    pub requests_accepted: u64,
    pub requests_rejected: u64,
    pub backend_errors: u64,
    pub p95_latency_ms: Option<u64>,
    pub pool: Option<PoolMetrics>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceProfile {
    pub samples: usize,
    pub min_latency_ms: Option<u64>,
    pub avg_latency_ms: Option<f64>,
    pub p50_latency_ms: Option<u64>,
    pub p95_latency_ms: Option<u64>,
    pub p99_latency_ms: Option<u64>,
    pub max_latency_ms: Option<u64>,
}

#[derive(Debug, Default)]
pub struct MetricsRegistry {
    requests_total: AtomicU64,
    requests_accepted: AtomicU64,
    requests_rejected: AtomicU64,
    backend_errors: AtomicU64,
    latencies_ms: Mutex<Vec<u64>>,
    pool: Mutex<Option<PoolMetrics>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_accepted(&self, latency_ms: u64) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_accepted.fetch_add(1, Ordering::Relaxed);
        self.record_latency(latency_ms);
    }

    pub fn record_rejected(&self, latency_ms: u64) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_rejected.fetch_add(1, Ordering::Relaxed);
        self.record_latency(latency_ms);
    }

    pub fn record_backend_error(&self) {
        self.backend_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_pool_metrics(&self, metrics: PoolMetrics) {
        if let Ok(mut guard) = self.pool.lock() {
            *guard = Some(metrics);
        }
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let p95_latency_ms = self
            .latencies_ms
            .lock()
            .ok()
            .and_then(|latencies| percentile_95(&latencies));
        let pool = self.pool.lock().ok().and_then(|guard| *guard);

        MetricsSnapshot {
            requests_total: self.requests_total.load(Ordering::Relaxed),
            requests_accepted: self.requests_accepted.load(Ordering::Relaxed),
            requests_rejected: self.requests_rejected.load(Ordering::Relaxed),
            backend_errors: self.backend_errors.load(Ordering::Relaxed),
            p95_latency_ms,
            pool,
        }
    }

    pub fn performance_profile(&self) -> PerformanceProfile {
        let latencies = self.latencies_ms.lock().ok().map(|guard| guard.clone());
        profile_from_latencies(latencies.unwrap_or_default())
    }

    fn record_latency(&self, latency_ms: u64) {
        if let Ok(mut latencies) = self.latencies_ms.lock() {
            latencies.push(latency_ms);
        }
    }
}

fn percentile_95(latencies: &[u64]) -> Option<u64> {
    if latencies.is_empty() {
        return None;
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() * 95).saturating_sub(1)) / 100;
    sorted.get(index).copied()
}

fn percentile(latencies: &[u64], percentile: usize) -> Option<u64> {
    if latencies.is_empty() {
        return None;
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() * percentile).saturating_sub(1)) / 100;
    sorted.get(index).copied()
}

fn profile_from_latencies(latencies: Vec<u64>) -> PerformanceProfile {
    let samples = latencies.len();
    let min_latency_ms = latencies.iter().min().copied();
    let max_latency_ms = latencies.iter().max().copied();
    let avg_latency_ms = if samples > 0 {
        let sum = latencies.iter().fold(0_u128, |accumulator, latency| {
            accumulator + u128::from(*latency)
        });
        Some(sum as f64 / samples as f64)
    } else {
        None
    };

    PerformanceProfile {
        samples,
        min_latency_ms,
        avg_latency_ms,
        p50_latency_ms: percentile(&latencies, 50),
        p95_latency_ms: percentile(&latencies, 95),
        p99_latency_ms: percentile(&latencies, 99),
        max_latency_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_p95_and_request_counters() {
        let metrics = MetricsRegistry::new();
        for latency in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            metrics.record_accepted(latency);
        }
        metrics.record_rejected(110);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 11);
        assert_eq!(snapshot.requests_accepted, 10);
        assert_eq!(snapshot.requests_rejected, 1);
        assert_eq!(snapshot.p95_latency_ms, Some(110));
    }

    #[test]
    fn records_backend_and_pool_metrics() {
        let metrics = MetricsRegistry::new();
        metrics.record_backend_error();
        metrics.record_backend_error();
        metrics.set_pool_metrics(PoolMetrics { size: 5, idle: 2 });

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.backend_errors, 2);
        assert_eq!(snapshot.pool, Some(PoolMetrics { size: 5, idle: 2 }));
    }

    #[test]
    fn returns_none_for_empty_latency_distribution() {
        let metrics = MetricsRegistry::new();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.p95_latency_ms, None);
    }

    #[test]
    fn computes_performance_profile_from_latencies() {
        let metrics = MetricsRegistry::new();
        for latency in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            metrics.record_accepted(latency);
        }
        let profile = metrics.performance_profile();
        assert_eq!(profile.samples, 10);
        assert_eq!(profile.min_latency_ms, Some(10));
        assert_eq!(profile.p50_latency_ms, Some(50));
        assert_eq!(profile.p95_latency_ms, Some(100));
        assert_eq!(profile.p99_latency_ms, Some(100));
        assert_eq!(profile.max_latency_ms, Some(100));
        assert_eq!(profile.avg_latency_ms, Some(55.0));
    }

    #[test]
    fn returns_empty_performance_profile_without_samples() {
        let metrics = MetricsRegistry::new();
        let profile = metrics.performance_profile();
        assert_eq!(profile.samples, 0);
        assert_eq!(profile.min_latency_ms, None);
        assert_eq!(profile.avg_latency_ms, None);
        assert_eq!(profile.p50_latency_ms, None);
        assert_eq!(profile.p95_latency_ms, None);
        assert_eq!(profile.p99_latency_ms, None);
        assert_eq!(profile.max_latency_ms, None);
    }
}
