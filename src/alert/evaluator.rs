use std::time::Instant;

use super::{AlertRule, AlertSeverity};
use crate::state::Snapshot;

/// Per-rule mutable evaluation state.
struct RuleState {
    rule: AlertRule,
    breach_since: Option<Instant>,
    firing: bool,
}

/// Outcome of one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertEventKind {
    /// Rule transitioned from not-firing to firing this tick.
    Fired,
    /// Rule transitioned from firing to not-firing this tick.
    Recovered,
}

#[derive(Debug, Clone)]
pub struct AlertEvent {
    pub rule_name: String,
    pub metric: String,
    pub severity: AlertSeverity,
    pub kind: AlertEventKind,
    pub at: Instant,
    /// Value that triggered this transition (current sample value).
    pub value: f64,
    pub threshold: f64,
}

/// What gets stamped into `Snapshot::firing_alerts` so the UI can colour
/// affected panel borders without re-running the evaluator.
#[derive(Debug, Clone)]
pub struct FiringAlert {
    pub rule_name: String,
    pub metric: String,
    pub severity: AlertSeverity,
    pub value: f64,
    pub threshold: f64,
}

pub struct AlertEvaluator {
    rules: Vec<RuleState>,
}

impl AlertEvaluator {
    pub fn new(rules: impl IntoIterator<Item = AlertRule>) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|rule| RuleState {
                    rule,
                    breach_since: None,
                    firing: false,
                })
                .collect(),
        }
    }

    /// Run all rules against `snapshot`. Returns transition events emitted
    /// this tick; `firing_now` is the full set of rules currently firing
    /// (caller copies into Snapshot for the UI).
    pub fn evaluate(&mut self, snapshot: &Snapshot, now: Instant) -> EvaluateOutput {
        let mut events = Vec::new();
        let mut firing_now = Vec::new();

        for rs in self.rules.iter_mut() {
            let value = match snapshot.get(&rs.rule.metric) {
                Some(v) => v,
                None => {
                    // Metric not present this tick — treat as not-in-breach
                    // (don't fire, don't recover from a previous fire either).
                    if rs.firing {
                        // The metric vanished while we were firing. Surface
                        // as a recovery so the user knows the alert isn't
                        // pinned forever.
                        rs.firing = false;
                        rs.breach_since = None;
                        events.push(AlertEvent {
                            rule_name: rs.rule.name.clone(),
                            metric: rs.rule.metric.clone(),
                            severity: rs.rule.severity,
                            kind: AlertEventKind::Recovered,
                            at: now,
                            value: f64::NAN,
                            threshold: rs.rule.value,
                        });
                    }
                    continue;
                }
            };

            let in_breach = rs.rule.op.eval(value, rs.rule.value);
            if in_breach {
                let since = *rs.breach_since.get_or_insert(now);
                if !rs.firing && now.duration_since(since) >= rs.rule.duration {
                    rs.firing = true;
                    events.push(AlertEvent {
                        rule_name: rs.rule.name.clone(),
                        metric: rs.rule.metric.clone(),
                        severity: rs.rule.severity,
                        kind: AlertEventKind::Fired,
                        at: now,
                        value,
                        threshold: rs.rule.value,
                    });
                }
            } else if rs.firing {
                rs.firing = false;
                rs.breach_since = None;
                events.push(AlertEvent {
                    rule_name: rs.rule.name.clone(),
                    metric: rs.rule.metric.clone(),
                    severity: rs.rule.severity,
                    kind: AlertEventKind::Recovered,
                    at: now,
                    value,
                    threshold: rs.rule.value,
                });
            } else {
                rs.breach_since = None;
            }

            if rs.firing {
                firing_now.push(FiringAlert {
                    rule_name: rs.rule.name.clone(),
                    metric: rs.rule.metric.clone(),
                    severity: rs.rule.severity,
                    value,
                    threshold: rs.rule.value,
                });
            }
        }

        EvaluateOutput { events, firing_now }
    }
}

#[derive(Debug, Default)]
pub struct EvaluateOutput {
    pub events: Vec<AlertEvent>,
    pub firing_now: Vec<FiringAlert>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::{AlertOp, AlertSeverity};
    use std::time::Duration;

    fn rule(name: &str, metric: &str, op: AlertOp, value: f64, dur_secs: u64) -> AlertRule {
        AlertRule {
            name: name.into(),
            metric: metric.into(),
            op,
            value,
            duration: Duration::from_secs(dur_secs),
            severity: AlertSeverity::Warn,
        }
    }

    fn snap_with(metric: &str, value: f64) -> Snapshot {
        let mut s = Snapshot::new();
        s.set(metric, value);
        s
    }

    #[test]
    fn fires_only_after_breach_persists_for_duration() {
        let mut eval = AlertEvaluator::new([rule("hot", "cpu.total", AlertOp::Gt, 90.0, 30)]);
        let t0 = Instant::now();
        // First breach observation — start the timer, don't fire yet.
        let r1 = eval.evaluate(&snap_with("cpu.total", 95.0), t0);
        assert!(r1.events.is_empty());
        assert!(r1.firing_now.is_empty());

        // Still in breach but not yet at duration.
        let r2 = eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(15));
        assert!(r2.events.is_empty());

        // Crosses duration → fire.
        let r3 = eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(30));
        assert_eq!(r3.events.len(), 1);
        assert_eq!(r3.events[0].kind, AlertEventKind::Fired);
        assert_eq!(r3.firing_now.len(), 1);
    }

    #[test]
    fn recovers_when_breach_clears() {
        let mut eval = AlertEvaluator::new([rule("hot", "cpu.total", AlertOp::Gt, 90.0, 0)]);
        let t0 = Instant::now();
        // duration=0 so it fires immediately.
        let r1 = eval.evaluate(&snap_with("cpu.total", 95.0), t0);
        assert_eq!(r1.events[0].kind, AlertEventKind::Fired);

        let r2 = eval.evaluate(&snap_with("cpu.total", 50.0), t0 + Duration::from_secs(5));
        assert_eq!(r2.events.len(), 1);
        assert_eq!(r2.events[0].kind, AlertEventKind::Recovered);
        assert!(r2.firing_now.is_empty());
    }

    #[test]
    fn flapping_breach_resets_breach_since() {
        let mut eval = AlertEvaluator::new([rule("hot", "cpu.total", AlertOp::Gt, 90.0, 30)]);
        let t0 = Instant::now();
        eval.evaluate(&snap_with("cpu.total", 95.0), t0);
        // Drop below before duration completes — breach_since should reset.
        eval.evaluate(&snap_with("cpu.total", 50.0), t0 + Duration::from_secs(15));
        // Back into breach — still shouldn't fire because the timer reset.
        eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(20));
        let r = eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(35));
        // Only 15s of continuous breach so far — shouldn't fire yet.
        assert!(r.events.is_empty());
    }

    #[test]
    fn missing_metric_recovers_active_alert() {
        let mut eval = AlertEvaluator::new([rule("hot", "cpu.total", AlertOp::Gt, 90.0, 0)]);
        let t0 = Instant::now();
        eval.evaluate(&snap_with("cpu.total", 95.0), t0);
        // Next snapshot lacks the metric.
        let r = eval.evaluate(&Snapshot::new(), t0 + Duration::from_secs(1));
        assert_eq!(r.events.len(), 1);
        assert_eq!(r.events[0].kind, AlertEventKind::Recovered);
        // Recovered events for missing metrics carry NaN — the widget
        // formats NaN specially. Don't assert == here.
        assert!(r.events[0].value.is_nan());
    }

    #[test]
    fn duration_zero_fires_on_first_breach() {
        // duration = 0 should fire immediately on the first sample that's
        // in breach, not require two ticks to accumulate.
        let mut eval = AlertEvaluator::new([rule("hot", "cpu.total", AlertOp::Gt, 90.0, 0)]);
        let r = eval.evaluate(&snap_with("cpu.total", 95.0), Instant::now());
        assert_eq!(r.events.len(), 1);
        assert_eq!(r.events[0].kind, AlertEventKind::Fired);
    }

    #[test]
    fn re_fires_after_recovery() {
        // Once recovered, a fresh breach must rebuild from breach_since=now
        // and fire again after duration.
        let mut eval = AlertEvaluator::new([rule("hot", "cpu.total", AlertOp::Gt, 90.0, 5)]);
        let t0 = Instant::now();
        eval.evaluate(&snap_with("cpu.total", 95.0), t0);
        eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(5));
        // recovered
        let recover = eval.evaluate(&snap_with("cpu.total", 50.0), t0 + Duration::from_secs(6));
        assert_eq!(recover.events[0].kind, AlertEventKind::Recovered);

        // back into breach — must wait the full duration again
        let mid = eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(7));
        assert!(mid.events.is_empty());
        let fired_again =
            eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(12));
        assert_eq!(fired_again.events.len(), 1);
        assert_eq!(fired_again.events[0].kind, AlertEventKind::Fired);
    }

    #[test]
    fn same_metric_two_rules_track_independently() {
        // Two rules with different durations against the same metric
        // shouldn't share state; each tracks its own breach_since.
        let mut eval = AlertEvaluator::new([
            rule("warn", "cpu.total", AlertOp::Gt, 80.0, 0),
            rule("hot", "cpu.total", AlertOp::Gt, 90.0, 5),
        ]);
        let t0 = Instant::now();
        let r1 = eval.evaluate(&snap_with("cpu.total", 95.0), t0);
        // warn fires immediately, hot doesn't yet.
        assert_eq!(r1.events.len(), 1);
        assert_eq!(r1.events[0].rule_name, "warn");
        assert_eq!(r1.firing_now.len(), 1);

        let r2 = eval.evaluate(&snap_with("cpu.total", 95.0), t0 + Duration::from_secs(5));
        // hot now also fires; warn keeps firing (no transition emitted).
        assert_eq!(r2.events.len(), 1);
        assert_eq!(r2.events[0].rule_name, "hot");
        assert_eq!(r2.firing_now.len(), 2);
    }
}
