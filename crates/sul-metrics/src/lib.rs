//! High-throughput telemetry rollup using `FxHashMap` (`rustc-hash`) and `bumpalo` scratch arenas (D-148).

#![forbid(unsafe_code)]

use rustc_hash::FxHashMap;
use sul_ipc_types::TelemetryMsg;

#[derive(Default)]
pub struct TelemetryRollup {
    counts: FxHashMap<String, u64>,
    totals_us: FxHashMap<String, u64>,
}

impl TelemetryRollup {
    pub fn record(&mut self, msg: &TelemetryMsg) {
        let key = format!("{}::{}", msg.subsystem, msg.metric);
        *self.counts.entry(key.clone()).or_insert(0) += 1;
        *self.totals_us.entry(key).or_insert(0) += msg.value_us;
    }

    pub fn mean_us(&self, subsystem: &str, metric: &str) -> Option<u64> {
        let key = format!("{subsystem}::{metric}");
        let count = *self.counts.get(&key)?;
        let total = *self.totals_us.get(&key)?;
        Some(total / count.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollup_computes_mean() {
        let mut r = TelemetryRollup::default();
        r.record(&TelemetryMsg {
            subsystem: "ai".into(),
            metric: "ttft".into(),
            value_us: 100,
            timestamp_ns: 1,
        });
        r.record(&TelemetryMsg {
            subsystem: "ai".into(),
            metric: "ttft".into(),
            value_us: 200,
            timestamp_ns: 2,
        });
        assert_eq!(r.mean_us("ai", "ttft"), Some(150));
    }
}
