use std::collections::{HashMap, VecDeque};

use super::snapshot::{MetricKey, Snapshot};

/// Per-metric ring buffer of recent values. Oldest at the front, newest at the
/// back. Capacity is shared by every series so memory is bounded at
/// `capacity * num_metrics` floats.
#[derive(Debug)]
pub struct History {
    capacity: usize,
    series: HashMap<MetricKey, VecDeque<f64>>,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            series: HashMap::new(),
        }
    }

    /// Append every numeric value from `snapshot` into its corresponding
    /// series, dropping the oldest sample when full.
    pub fn push_from(&mut self, snapshot: &Snapshot) {
        for (key, value) in &snapshot.numeric {
            let buf = self
                .series
                .entry(key.clone())
                .or_insert_with(|| VecDeque::with_capacity(self.capacity));
            if buf.len() == self.capacity {
                buf.pop_front();
            }
            buf.push_back(*value);
        }
    }

    pub fn series(&self, key: &str) -> Option<&VecDeque<f64>> {
        self.series.get(&MetricKey::new(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap_with(key: &str, v: f64) -> Snapshot {
        let mut s = Snapshot::new();
        s.set(key, v);
        s
    }

    #[test]
    fn push_appends_and_caps_at_capacity() {
        let mut h = History::new(3);
        for v in [1.0, 2.0, 3.0, 4.0] {
            h.push_from(&snap_with("k", v));
        }
        let s = h.series("k").unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn missing_series_is_none() {
        let h = History::new(2);
        assert!(h.series("nope").is_none());
    }

    #[test]
    fn capacity_zero_is_clamped_to_one() {
        let mut h = History::new(0);
        h.push_from(&snap_with("k", 9.0));
        assert_eq!(h.series("k").unwrap().len(), 1);
    }
}
