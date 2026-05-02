mod history;
mod snapshot;

pub use history::History;
pub use snapshot::{
    BatteryReading, BatteryStatus, ContainerSnapshot, DiskSnapshot, NetworkSnapshot,
    ProcessSnapshot, SensorReading, Snapshot,
};

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use crate::alert::AlertEvent;

/// Maximum number of recent alert transitions retained in `StateInner`.
/// The overlay shows them in reverse chronological order.
const ALERT_EVENT_CAPACITY: usize = 50;

pub type SharedState = Arc<State>;

/// Central shared state. Sampler writes; UI and exporter read.
pub struct State {
    inner: RwLock<StateInner>,
}

struct StateInner {
    current: Option<Snapshot>,
    history: History,
    /// Ring buffer of recent alert transitions (Fired / Recovered).
    /// Newest at the back.
    alert_events: VecDeque<AlertEvent>,
}

impl State {
    pub fn new(history_capacity: usize) -> Self {
        Self {
            inner: RwLock::new(StateInner {
                current: None,
                history: History::new(history_capacity),
                alert_events: VecDeque::with_capacity(ALERT_EVENT_CAPACITY),
            }),
        }
    }

    /// Replace the current snapshot, append to history, and append any new
    /// alert transitions emitted this tick.
    pub fn commit(&self, snapshot: Snapshot, new_events: Vec<AlertEvent>) {
        let mut guard = self.inner.write().expect("state poisoned");
        guard.history.push_from(&snapshot);
        for ev in new_events {
            if guard.alert_events.len() == ALERT_EVENT_CAPACITY {
                guard.alert_events.pop_front();
            }
            guard.alert_events.push_back(ev);
        }
        guard.current = Some(snapshot);
    }

    /// Run a closure with read access to the latest snapshot and history.
    pub fn with_view<R>(&self, f: impl FnOnce(StateView<'_>) -> R) -> R {
        let guard = self.inner.read().expect("state poisoned");
        let view = StateView {
            current: guard.current.as_ref(),
            history: &guard.history,
            alert_events: &guard.alert_events,
        };
        f(view)
    }
}

pub struct StateView<'a> {
    pub current: Option<&'a Snapshot>,
    pub history: &'a History,
    /// Recent alert transitions; oldest at the front, newest at the back.
    pub alert_events: &'a VecDeque<AlertEvent>,
}
