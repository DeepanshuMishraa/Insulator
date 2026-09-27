//! Desktop ownership of the Insulator daemon process.

use std::path::PathBuf;

use anyhow::{Context as _, bail};

pub fn start_process() -> anyhow::Result<insulator_client::DaemonSupervisor> {
    let address = std::env::var(insulator_client::DAEMON_ADDRESS_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let token = std::env::var(insulator_client::DAEMON_TOKEN_ENV)
        .ok()
        .filter(|value| !value.is_empty());
    match (address, token) {
        (Some(address), Some(token)) => {
            return insulator_client::DaemonSupervisor::connect(address.trim(), token);
        }
        (Some(_), None) => bail!(
            "{} is set but {} is missing",
            insulator_client::DAEMON_ADDRESS_ENV,
            insulator_client::DAEMON_TOKEN_ENV
        ),
        (None, Some(_)) => bail!(
            "{} is set but {} is missing",
            insulator_client::DAEMON_TOKEN_ENV,
            insulator_client::DAEMON_ADDRESS_ENV
        ),
        (None, None) => {}
    }
    let app_settings = insulator_client::persistence::load_or_create_app_settings()
        .context("could not load desktop daemon settings")?;
    insulator_client::DaemonSupervisor::spawn_configured(
        &daemon_executable_path()?,
        cfg!(debug_assertions),
        app_settings.daemon_exposure,
    )
}

/// Resolve the local host name once during app construction. Settings can
/// then show a useful LAN URL without touching the OS from a render frame.
pub fn local_hostname() -> Option<String> {    #[cfg(unix)]
    {
        let mut buffer = [0_u8; 256];
        let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if result == 0 {
            let length = buffer
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(buffer.len());
            let hostname = String::from_utf8_lossy(&buffer[..length]).trim().to_owned();
            if !hostname.is_empty() {
                return Some(hostname);
            }
        }
    }
    // `COMPUTERNAME` is the Windows equivalent and is always set; `HOSTNAME`
    // covers the shells that export it.
    ["COMPUTERNAME", "HOSTNAME"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|hostname| hostname.trim().to_owned())
        .find(|hostname| !hostname.is_empty())
}

/// First IPv4 address in `tailscale ip` output, preferring Tailnet (100.x)
/// addresses. Pure parsing so it stays unit-testable.
pub fn parse_tailscale_ipv4(output: &str) -> Option<String> {
    let mut fallback = None;
    for line in output.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.parse::<std::net::Ipv4Addr>().is_ok() {
            if line.starts_with("100.") {
                return Some(line.to_owned());
            }
            fallback.get_or_insert_with(|| line.to_owned());
        }
    }
    fallback
}

/// Resolve this machine's Tailscale IPv4.
///
/// Primary path reads interface addresses directly (no subprocess, no PATH
/// lookup — GUI apps do not inherit the shell's PATH, so `tailscale` is
/// often unresolvable there). `tailscale ip -4` remains as a fallback for
/// exotic setups. Blocking at worst — invoke from a background executor.
pub fn tailscale_ipv4() -> Option<String> {
    local_tailscale_ipv4().or_else(tailscale_cli_ipv4)
}

/// Tailscale assigns addresses from the shared CGNAT range 100.64.0.0/10.
/// No other local interface should hold one, so the first match wins.
#[cfg(unix)]
fn local_tailscale_ipv4() -> Option<String> {
    // SAFETY: getifaddrs hands a linked list freed exactly once via the
    // guard below; each ifa_addr is read only while the list is alive.
    unsafe {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut list) != 0 || list.is_null() {
            return None;
        }
        struct ListGuard(*mut libc::ifaddrs);
        impl Drop for ListGuard {
            fn drop(&mut self) {
                // SAFETY: paired with the successful getifaddrs above.
                unsafe { libc::freeifaddrs(self.0) };
            }
        }
        let _guard = ListGuard(list);
        let mut current = list;
        while !current.is_null() {
            let entry = &*current;
            if !entry.ifa_addr.is_null()
                && (*entry.ifa_addr).sa_family as i32 == libc::AF_INET
            {
                let raw = &*(entry.ifa_addr as *const libc::sockaddr_in);
                // s_addr is network byte order, which is also memory order.
                let octets = std::net::Ipv4Addr::from(raw.sin_addr.s_addr.to_ne_bytes()).octets();
                if octets[0] == 100 && (64..=127).contains(&octets[1]) {
                    return Some(std::net::Ipv4Addr::from(octets).to_string());
                }
            }
            current = (*current).ifa_next;
        }
        None
    }
}

#[cfg(not(unix))]
fn local_tailscale_ipv4() -> Option<String> {
    None
}

fn tailscale_cli_ipv4() -> Option<String> {
    // The bare name covers shells and service managers with a full PATH;
    // the absolute paths cover GUI apps (Homebrew installs).
    for binary in [
        "tailscale",
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
    ] {
        let output = std::process::Command::new(binary)
            .arg("ip")
            .arg("-4")
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        if let Some(address) = parse_tailscale_ipv4(&String::from_utf8_lossy(&output.stdout)) {
            return Some(address);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_output_prefers_tailnet_ipv4() {
        assert_eq!(
            parse_tailscale_ipv4("100.64.245.111\nfd7a:115c:a1e0::4a01:f596\n"),
            Some("100.64.245.111".into())
        );
    }

    #[test]
    fn tailscale_output_skips_ipv6_and_blanks() {
        assert_eq!(
            parse_tailscale_ipv4("\nfd7a:115c:a1e0::1\n100.99.0.2\n"),
            Some("100.99.0.2".into())
        );
    }

    #[test]
    fn tailscale_output_without_ipv4_resolves_to_none() {
        assert_eq!(parse_tailscale_ipv4(""), None);
        assert_eq!(parse_tailscale_ipv4("fd7a:115c:a1e0::1\n"), None);
    }
}

fn daemon_executable_path() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("INSULATOR_DAEMON_PATH")
        .or_else(|| std::env::var_os("INSULATOR_DAEMON_PATH"))
        .filter(|path| !path.is_empty())
    {
        return Ok(path.into());
    }
    let candidates = [
        format!("insulator-debug-daemon{}", std::env::consts::EXE_SUFFIX),
        format!("insulator-daemon{}", std::env::consts::EXE_SUFFIX),
        format!("insulator-daemon{}", std::env::consts::EXE_SUFFIX),
    ];
    let current = std::env::current_exe().context("could not locate the app executable")?;

    // Development keeps the daemon beside Cargo's debug artifacts rather than
    // inside Insulator Debug.app. The supervisor watches this file and swaps only
    // the daemon when the development watcher relinks it.
    #[cfg(debug_assertions)]
    if let Some(debug_directory) = current
        .ancestors()
        .find(|candidate| candidate.file_name().is_some_and(|name| name == "debug"))
    {
        for candidate in &candidates {
            let external = debug_directory.join(candidate);
            if external.is_file() {
                return Ok(external);
            }
        }
    }

    if let Some(parent) = current.parent() {
        for candidate in &candidates {
            let sibling = parent.join(candidate);
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }

    let default_sibling = current
        .parent()
        .map(|directory| directory.join(&candidates[0]))
        .unwrap_or_else(|| PathBuf::from(&candidates[0]));

    #[cfg(debug_assertions)]
    bail!(
        "Insulator daemon was not found in Cargo's debug directory or next to the app executable: {}",
        default_sibling.display(),
    );
    #[cfg(not(debug_assertions))]
    bail!(
        "Insulator daemon is missing next to the app executable: {}",
        default_sibling.display(),
    )
}
