mod history;
mod snapshot;

pub use history::History;
pub use snapshot::{
    BatteryReading, BatteryStatus, DiskSnapshot, NetworkSnapshot, ProcessSnapshot, SensorReading,
    Snapshot,
};

use std::sync::{Arc, RwLock};

pub type SharedState = Arc<State>;

/// Central shared state. Sampler writes; UI and exporter read.
pub struct State {
    inner: RwLock<StateInner>,
}

struct StateInner {
    current: Option<Snapshot>,
    history: History,
}

impl State {
    pub fn new(history_capacity: usize) -> Self {
        Self {
            inner: RwLock::new(StateInner {
                current: None,
                history: History::new(history_capacity),
            }),
        }
    }

    /// Replace the current snapshot and append to history.
    pub fn commit(&self, snapshot: Snapshot) {
        let mut guard = self.inner.write().expect("state poisoned");
        guard.history.push_from(&snapshot);
        guard.current = Some(snapshot);
    }

    /// Run a closure with read access to the latest snapshot and history.
    pub fn with_view<R>(&self, f: impl FnOnce(StateView<'_>) -> R) -> R {
        let guard = self.inner.read().expect("state poisoned");
        let view = StateView {
            current: guard.current.as_ref(),
            history: &guard.history,
        };
        f(view)
    }
}

pub struct StateView<'a> {
    pub current: Option<&'a Snapshot>,
    pub history: &'a History,
}
