/// Format a byte count as a short human-readable string (`12.4G`, `512K`).
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut value = n as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{n}{}", UNITS[idx])
    } else if value >= 100.0 {
        format!("{value:.0}{}", UNITS[idx])
    } else if value >= 10.0 {
        format!("{value:.1}{}", UNITS[idx])
    } else {
        format!("{value:.2}{}", UNITS[idx])
    }
}

/// Format a duration in seconds as `Hh:Mm:Ss` (or `Dd Hh:Mm` if very long).
pub fn run_time(secs: u64) -> String {
    let days = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{days}d {h:02}:{m:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_units_step() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(512), "512B");
        assert_eq!(bytes(2048), "2.00K");
        assert_eq!(bytes(15 * 1024), "15.0K");
        assert_eq!(bytes(150 * 1024), "150K");
        assert_eq!(bytes(2 * 1024 * 1024 * 1024), "2.00G");
    }

    #[test]
    fn run_time_short_and_long() {
        assert_eq!(run_time(0), "00:00:00");
        assert_eq!(run_time(65), "00:01:05");
        assert_eq!(run_time(3661), "01:01:01");
        assert_eq!(run_time(90_061), "1d 01:01");
    }
}
