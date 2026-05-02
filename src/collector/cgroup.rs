//! Read `/proc/<pid>/cgroup` to determine which container (if any) a
//! process belongs to. Linux-only; on other platforms the public entry
//! point is a stub that returns `None` for every PID.

#[cfg(target_os = "linux")]
pub fn read_pid_container(pid: u32) -> Option<String> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    parse_cgroup(&content)
}

#[cfg(not(target_os = "linux"))]
pub fn read_pid_container(_pid: u32) -> Option<String> {
    None
}

/// Walk every line of a /proc/<pid>/cgroup file, return the first
/// container ID we recognise. Supports:
///
/// - cgroup v2 systemd: `0::/system.slice/docker-<64hex>.scope`
/// - cgroup v2 systemd k8s: `0::/.../cri-containerd-<64hex>.scope`
/// - cgroup v1 docker: `12:cpu,cpuacct:/docker/<64hex>`
///
/// Available on all platforms so unit tests can run on the macOS CI
/// runner; the public entry above is what gates by target_os.
#[cfg(any(test, target_os = "linux"))]
fn parse_cgroup(content: &str) -> Option<String> {
    for line in content.lines() {
        // Each line is "id:subsystems:path" — we only care about path.
        let path = match line.rsplit_once(':') {
            Some((_, p)) => p,
            None => continue,
        };
        if let Some(id) = extract_container_id(path) {
            return Some(id);
        }
    }
    None
}

#[cfg(any(test, target_os = "linux"))]
fn extract_container_id(path: &str) -> Option<String> {
    // Try in order of specificity. Each pattern: prefix → run of hex chars.
    for prefix in ["docker-", "cri-containerd-"] {
        if let Some(after) = find_after(path, prefix) {
            let id: String = after.chars().take_while(char::is_ascii_hexdigit).collect();
            if id.len() >= 12 {
                return Some(id);
            }
        }
    }
    if let Some(after) = find_after(path, "/docker/") {
        let id: String = after.chars().take_while(char::is_ascii_hexdigit).collect();
        if id.len() >= 12 {
            return Some(id);
        }
    }
    None
}

#[cfg(any(test, target_os = "linux"))]
fn find_after<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack.find(needle).map(|i| &haystack[i + needle.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgroup_v2_systemd_docker() {
        let s = "0::/system.slice/docker-abc123def456789012345678901234567890123456789012345678901234567890.scope\n";
        assert_eq!(
            parse_cgroup(s),
            Some("abc123def456789012345678901234567890123456789012345678901234567890".to_string())
        );
    }

    #[test]
    fn cgroup_v2_kubernetes_containerd() {
        let s = "0::/kubepods.slice/kubepods-burstable.slice/kubepods-burstable-pod1234.slice/cri-containerd-deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef.scope\n";
        assert_eq!(
            parse_cgroup(s),
            Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string())
        );
    }

    #[test]
    fn cgroup_v1_docker() {
        let s = "12:cpu,cpuacct:/docker/abc123def456789012345678901234567890123456789012345678901234567890\n11:memory:/docker/abc123def456789012345678901234567890123456789012345678901234567890\n";
        assert_eq!(
            parse_cgroup(s),
            Some("abc123def456789012345678901234567890123456789012345678901234567890".to_string())
        );
    }

    #[test]
    fn no_container_user_session() {
        let s = "0::/user.slice/user-1000.slice/user@1000.service/app.slice\n";
        assert_eq!(parse_cgroup(s), None);
    }

    #[test]
    fn no_container_system_service() {
        let s = "0::/system.slice/sshd.service\n";
        assert_eq!(parse_cgroup(s), None);
    }

    #[test]
    fn rejects_short_hex_run() {
        // less than 12 hex chars after the prefix = treat as no match
        let s = "0::/system.slice/docker-abc.scope\n";
        assert_eq!(parse_cgroup(s), None);
    }
}
