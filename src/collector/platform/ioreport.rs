//! macOS Apple Silicon power readings via the private IOReport framework.
//! Avoids `sudo powermetrics` for what it can.
//!
//! IOReport is unsupported / undocumented; channel names and signatures
//! are reverse-engineered (cf. open-source projects like macmon, asitop).
//! Treat anything in here as best-effort — sample readings can disappear
//! between macOS releases, and we degrade silently to an empty
//! `Vec<SensorReading>` rather than failing the sensor pipeline.
//!
//! Phase B scope: CPU / GPU / ANE energy → derived power (W). True die
//! temperatures aren't consistently exposed via IOReport on Apple Silicon
//! (they're behind PMP / SMC channels that gate by chip generation), so
//! they're explicitly out of scope and tracked in TODO.md.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::time::Instant;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};

use crate::state::SensorReading;

/// All IOReport API parameters and return values are opaque CF-typed
/// pointers; we avoid trying to bind them to specific concrete types
/// because the reverse-engineered signatures vary across descriptions.
type Opaque = *const c_void;

// macOS 26 (Tahoe) moved IOReport from
// /System/Library/PrivateFrameworks/IOReport.framework into the dyld
// shared cache as /usr/lib/libIOReport.dylib. Link as a plain dylib so
// we don't depend on the `framework` form (and on a CLT SDK that ships
// an IOReport stub).
#[link(name = "IOReport")]
unsafe extern "C" {
    fn IOReportCopyChannelsInGroup(
        group: CFStringRef,
        subgroup: CFStringRef,
        channel_id: u64,
        a: u64,
        b: u64,
    ) -> Opaque;

    /// Reverse-engineered signature (cf. macmon): first arg is unused
    /// (always NULL), `channels` from `IOReportCopyChannelsInGroup` goes
    /// in the second slot, the third is an out-param for the subscribed
    /// dict, then channel id (0), then a cookie (NULL). Getting the arg
    /// order wrong here SIGTRAPs the process at first call.
    fn IOReportCreateSubscription(
        unused_a: Opaque,
        channels: Opaque,
        subscribed_channels: *mut Opaque,
        channel_id: u64,
        cookie: CFTypeRef,
    ) -> Opaque;

    fn IOReportCreateSamples(
        subscription: Opaque,
        subscribed_channels: Opaque,
        nil: CFTypeRef,
    ) -> Opaque;

    fn IOReportCreateSamplesDelta(prev: Opaque, current: Opaque, nil: CFTypeRef) -> Opaque;

    fn IOReportSimpleGetIntegerValue(channel: Opaque, index: i32) -> i64;
    fn IOReportChannelGetChannelName(channel: Opaque) -> CFStringRef;
}

/// Stateful sampler. Owns the subscription + previous full sample so
/// `sample()` can compute deltas. Subscription is created once on `new()`.
pub struct IoReportSampler {
    subscription: Opaque,
    /// Channels container from `IOReportCopyChannelsInGroup` (Create rule).
    channels: Opaque,
    /// Out-param dict written by `IOReportCreateSubscription` — also a
    /// Create-rule reference we have to release. Not used for anything
    /// else on our side; just kept alive for symmetry.
    subscribed_channels: Opaque,
    /// Previous full sample paired with its capture time so a transient
    /// failure (and the resulting cleared state) can't produce a stale
    /// delta with a wildly long `dt_secs`.
    prev: Option<(Opaque, Instant)>,
}

// SAFETY: All IOReport handles are pointers; we own them and only drop in
// `Drop`. Sampler lives on a single thread (the collector poller); `Send`
// is here only so we can stash it inside `SensorsCollector`. We don't
// implement `Sync` — concurrent `&self` use is not supported.
unsafe impl Send for IoReportSampler {}

impl IoReportSampler {
    /// Subscribe to the "Energy Model" group, which on Apple Silicon
    /// surfaces CPU / GPU / ANE energy counters.
    pub fn new() -> Option<Self> {
        let energy_group = CFString::new("Energy Model");
        let channels = unsafe {
            IOReportCopyChannelsInGroup(
                energy_group.as_concrete_TypeRef(),
                std::ptr::null(),
                0,
                0,
                0,
            )
        };
        if channels.is_null() {
            tracing::debug!("IOReport: no channels in Energy Model group");
            return None;
        }

        let mut subscribed_channels: Opaque = std::ptr::null();
        let subscription = unsafe {
            IOReportCreateSubscription(
                std::ptr::null(),
                channels,
                &mut subscribed_channels as *mut Opaque,
                0,
                std::ptr::null(),
            )
        };
        if subscription.is_null() {
            tracing::debug!("IOReport: CreateSubscription failed");
            unsafe { CFRelease(channels as CFTypeRef) };
            return None;
        }

        Some(Self {
            subscription,
            channels,
            subscribed_channels,
            prev: None,
        })
    }

    /// Take one sample, diff against the previous, and emit derived
    /// power readings (Watts). Returns an empty Vec on the very first
    /// call (need a delta to convert energy → power) or when the
    /// IOReport sampling call returns null.
    pub fn sample(&mut self) -> Vec<SensorReading> {
        let now = Instant::now();
        let raw_sample =
            unsafe { IOReportCreateSamples(self.subscription, self.channels, std::ptr::null()) };
        if raw_sample.is_null() {
            // Clear stale prev so we don't compute a delta with a long
            // dt_secs the next time sampling succeeds.
            if let Some((prev, _)) = self.prev.take() {
                unsafe { CFRelease(prev as CFTypeRef) };
            }
            return Vec::new();
        }

        let mut out = Vec::new();
        if let Some((prev, prev_at)) = self.prev.take() {
            let delta = unsafe { IOReportCreateSamplesDelta(prev, raw_sample, std::ptr::null()) };
            unsafe { CFRelease(prev as CFTypeRef) };
            if !delta.is_null() {
                let dt_secs = now.duration_since(prev_at).as_secs_f64().max(0.001);
                out = extract_power_readings(delta, dt_secs);
            }
        }

        self.prev = Some((raw_sample, now));
        out
    }
}

impl Drop for IoReportSampler {
    fn drop(&mut self) {
        if let Some((prev, _)) = self.prev.take() {
            unsafe { CFRelease(prev as CFTypeRef) };
        }
        unsafe {
            if !self.subscribed_channels.is_null() {
                CFRelease(self.subscribed_channels as CFTypeRef);
            }
            CFRelease(self.channels as CFTypeRef);
            CFRelease(self.subscription as CFTypeRef);
        }
    }
}

/// Walk the IOReportSampleArray inside `delta` and pull integer values for
/// every channel, converting nanojoules to Watts using `dt_secs`.
///
/// Takes ownership of `delta` (a Create-rule pointer). The CFDictionary
/// wrapper releases on drop, so we don't need a separate CFRelease.
fn extract_power_readings(delta: Opaque, dt_secs: f64) -> Vec<SensorReading> {
    let dict: CFDictionary =
        unsafe { CFDictionary::wrap_under_create_rule(delta as CFDictionaryRef) };
    let key = CFString::new("IOReportChannels");
    let raw = match dict.find(key.as_concrete_TypeRef() as *const c_void) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    if raw.is_null() {
        return Vec::new();
    }

    // The array reference is owned by the dict — wrap_under_get_rule takes
    // a +1 retain so it remains valid even after `dict` drops.
    let channels: CFArray<CFTypeRef> = unsafe { CFArray::wrap_under_get_rule(raw as CFArrayRef) };

    let mut out = Vec::new();
    for i in 0..channels.len() {
        let Some(channel_ptr) = channels.get(i) else {
            continue;
        };
        let channel: Opaque = *channel_ptr as Opaque;
        if channel.is_null() {
            continue;
        }
        let name_ref = unsafe { IOReportChannelGetChannelName(channel) };
        if name_ref.is_null() {
            continue;
        }
        let name = unsafe { CFString::wrap_under_get_rule(name_ref) }.to_string();
        let nanojoules = unsafe { IOReportSimpleGetIntegerValue(channel, 0) } as f64;
        // Counter wrap defense: skip negative readings, but emit zero
        // (e.g. ANE idle) so the panel reflects real state.
        if nanojoules < 0.0 {
            continue;
        }
        let watts = nanojoules / 1e9 / dt_secs;
        out.push(SensorReading {
            category: "power".to_string(),
            name: short_name(&name).to_string(),
            value: watts,
            unit: "W",
        });
    }

    out
}

/// IOReport channel names like "CPU Energy" / "GPU Energy" pack the unit
/// into the name. Strip the trailing "Energy" so the sensor label fits
/// the existing widget formatting.
fn short_name(channel_name: &str) -> &str {
    channel_name.strip_suffix(" Energy").unwrap_or(channel_name)
}
