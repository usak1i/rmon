//! Threshold alerts evaluated against `Snapshot::numeric` once per tick.
//!
//! Rules are loaded from TOML at startup (see `Config::alerts`). The
//! evaluator tracks per-rule "in breach since" timestamps so a rule only
//! fires once the breach has lasted for `duration`. Recovery (going back
//! out of breach) is also a transition the UI cares about — both surface
//! as `AlertEvent`s.

mod evaluator;

pub use evaluator::{AlertEvaluator, AlertEvent, AlertEventKind, FiringAlert};

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertOp {
    Gt,
    Lt,
    Ge,
    Le,
}

impl AlertOp {
    pub fn eval(self, lhs: f64, rhs: f64) -> bool {
        match self {
            AlertOp::Gt => lhs > rhs,
            AlertOp::Lt => lhs < rhs,
            AlertOp::Ge => lhs >= rhs,
            AlertOp::Le => lhs <= rhs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    #[default]
    Warn,
    Critical,
}

/// A single configured alert rule, in fully-validated form.
#[derive(Debug, Clone)]
pub struct AlertRule {
    pub name: String,
    pub metric: String,
    pub op: AlertOp,
    pub value: f64,
    pub duration: Duration,
    pub severity: AlertSeverity,
}

/// Raw TOML shape — strings for `op` and `duration` so the user gets a
/// readable config. Convert into `AlertRule` via `try_from`.
#[derive(Debug, Clone, Deserialize)]
pub struct AlertRuleRaw {
    pub name: String,
    pub metric: String,
    pub op: String,
    pub value: f64,
    pub duration: String,
    #[serde(default)]
    pub severity: AlertSeverity,
}

impl TryFrom<AlertRuleRaw> for AlertRule {
    type Error = anyhow::Error;

    fn try_from(raw: AlertRuleRaw) -> Result<Self> {
        let op = parse_op(&raw.op)
            .with_context(|| format!("alert {:?}: invalid op {:?}", raw.name, raw.op))?;
        let duration = parse_duration(&raw.duration).with_context(|| {
            format!("alert {:?}: invalid duration {:?}", raw.name, raw.duration)
        })?;
        Ok(Self {
            name: raw.name,
            metric: raw.metric,
            op,
            value: raw.value,
            duration,
            severity: raw.severity,
        })
    }
}

fn parse_op(s: &str) -> Result<AlertOp> {
    Ok(match s.trim() {
        ">" => AlertOp::Gt,
        "<" => AlertOp::Lt,
        ">=" => AlertOp::Ge,
        "<=" => AlertOp::Le,
        other => bail!("expected one of >,<,>=,<=; got {:?}", other),
    })
}

/// Parse a short human duration: `30s`, `5m`, `1h`. Single unit only.
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    let split = s
        .find(|c: char| c.is_ascii_alphabetic())
        .with_context(|| format!("missing unit in duration {:?}", s))?;
    let (num_part, unit) = s.split_at(split);
    let n: u64 = num_part
        .parse()
        .with_context(|| format!("duration {:?} prefix {:?} is not a number", s, num_part))?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        other => bail!("unknown duration unit {:?}", other),
    };
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_op_known() {
        assert_eq!(parse_op(">").unwrap(), AlertOp::Gt);
        assert_eq!(parse_op("<=").unwrap(), AlertOp::Le);
        assert!(parse_op("=>").is_err());
    }

    #[test]
    fn alert_op_eval() {
        assert!(AlertOp::Gt.eval(91.0, 90.0));
        assert!(!AlertOp::Gt.eval(90.0, 90.0));
        assert!(AlertOp::Ge.eval(90.0, 90.0));
        assert!(AlertOp::Lt.eval(0.5, 1.0));
        assert!(AlertOp::Le.eval(0.5, 1.0));
        assert!(AlertOp::Le.eval(1.0, 1.0));
    }

    #[test]
    fn parse_duration_units() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn parse_duration_rejects_bad_input() {
        assert!(parse_duration("30").is_err()); // no unit
        assert!(parse_duration("30d").is_err()); // unknown unit
        assert!(parse_duration("foo").is_err()); // no number
    }

    #[test]
    fn convert_raw_rule() {
        let raw = AlertRuleRaw {
            name: "cpu hot".into(),
            metric: "cpu.total".into(),
            op: ">".into(),
            value: 90.0,
            duration: "30s".into(),
            severity: AlertSeverity::Warn,
        };
        let r: AlertRule = raw.try_into().unwrap();
        assert_eq!(r.op, AlertOp::Gt);
        assert_eq!(r.duration, Duration::from_secs(30));
        assert_eq!(r.severity, AlertSeverity::Warn);
    }

    #[test]
    fn convert_raw_rule_bad_op_surfaces_name() {
        let raw = AlertRuleRaw {
            name: "broken".into(),
            metric: "cpu.total".into(),
            op: "??".into(),
            value: 1.0,
            duration: "1s".into(),
            severity: AlertSeverity::Info,
        };
        let err = AlertRule::try_from(raw).unwrap_err();
        assert!(err.to_string().contains("broken"));
    }
}
