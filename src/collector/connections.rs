//! TCP/UDP connection counts on Linux. macOS has no equivalent
//! sysfs/procfs surface; the only options there are slow/privileged tools
//! (`lsof -i`, `netstat`), tracked under TODO Phase 2.5 carryovers.

#[derive(Debug, Default, Clone, Copy)]
pub struct ConnCounts {
    pub tcp_established: u32,
    pub tcp_listen: u32,
    pub tcp_time_wait: u32,
    pub udp: u32,
}

#[cfg(target_os = "linux")]
pub fn read_counts() -> ConnCounts {
    use std::fs;
    let mut out = ConnCounts::default();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = fs::read_to_string(path) {
            let (e, l, w) = parse_tcp(&content);
            out.tcp_established += e;
            out.tcp_listen += l;
            out.tcp_time_wait += w;
        }
    }
    for path in ["/proc/net/udp", "/proc/net/udp6"] {
        if let Ok(content) = fs::read_to_string(path) {
            out.udp += parse_udp_count(&content);
        }
    }
    out
}

#[cfg(not(target_os = "linux"))]
pub fn read_counts() -> ConnCounts {
    ConnCounts::default()
}

/// Sum TCP rows by state column. State codes per linux `include/net/tcp_states.h`:
/// `01 = ESTABLISHED`, `06 = TIME_WAIT`, `0A = LISTEN`.
///
/// Available on any platform so tests can run on macOS CI even though the
/// caller is Linux-only.
#[cfg(any(test, target_os = "linux"))]
fn parse_tcp(content: &str) -> (u32, u32, u32) {
    let mut est = 0;
    let mut listen = 0;
    let mut tw = 0;
    for line in content.lines().skip(1) {
        match line.split_whitespace().nth(3) {
            Some("01") => est += 1,
            Some("06") => tw += 1,
            Some("0A") => listen += 1,
            _ => {}
        }
    }
    (est, listen, tw)
}

#[cfg(any(test, target_os = "linux"))]
fn parse_udp_count(content: &str) -> u32 {
    content
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP_FIXTURE: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0277 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12345 1 ...\n   1: 0100007F:0050 0100007F:9F2A 01 00000000:00000000 00:00000000 00000000  1000        0 67890 1 ...\n   2: 0100007F:9F2A 0100007F:0050 06 00000000:00000000 00:00000000 00000000  1000        0 67891 1 ...\n   3: 0100007F:8000 0100007F:0050 01 00000000:00000000 00:00000000 00000000  1000        0 67892 1 ...\n";

    #[test]
    fn parse_tcp_counts_states() {
        let (est, lis, tw) = parse_tcp(TCP_FIXTURE);
        assert_eq!(est, 2);
        assert_eq!(lis, 1);
        assert_eq!(tw, 1);
    }

    #[test]
    fn parse_tcp_handles_empty() {
        let (e, l, w) = parse_tcp("");
        assert_eq!((e, l, w), (0, 0, 0));
    }

    #[test]
    fn parse_udp_skips_header_and_blanks() {
        let s = "  sl ...\n   0: ...\n   1: ...\n\n";
        assert_eq!(parse_udp_count(s), 2);
    }
}
