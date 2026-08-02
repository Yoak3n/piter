
// ─── LAN IP Discovery ──────────────────────────────────────────────────────

pub fn discover_lan_ips() -> Vec<String> {
    let mut ips: Vec<String> = Vec::new();

    // Primary: UDP socket trick — connect to public DNS to find route IP
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:53").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip();
                if is_private_ipv4(ip) {
                    ips.push(ip.to_string());
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // ipconfig is a console app; without CREATE_NO_WINDOW a console
        // window flashes when spawned from the GUI-subsystem piter.exe.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(output) = std::process::Command::new("ipconfig")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.contains("IPv4") && line.contains(':') {
                    if let Some(ip_str) = line.split(':').next_back() {
                        let ip_str = ip_str.trim();
                        if let Ok(addr) = ip_str.parse::<std::net::IpAddr>() {
                            if is_private_ipv4(addr) && !ips.contains(&ip_str.to_string()) {
                                ips.push(ip_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for (cmd, args) in [("ifconfig", &["-a"] as &[&str]), ("ip", &["addr"])] {
            if let Ok(output) = std::process::Command::new(cmd).args(args).output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if let Some(inet) = trimmed.strip_prefix("inet ") {
                        if let Some(ip_part) = inet.split_whitespace().next() {
                            if let Ok(addr) = ip_part.parse::<std::net::IpAddr>() {
                                if is_private_ipv4(addr) && !ips.contains(&ip_part.to_string()) {
                                    ips.push(ip_part.to_string());
                                }
                            }
                        }
                    }
                }
                if !ips.is_empty() {
                    break;
                }
            }
        }
    }

    ips
}

fn is_private_ipv4(ip: std::net::IpAddr) -> bool {
    if !ip.is_ipv4() || ip.is_loopback() {
        return false;
    }
    match ip {
        std::net::IpAddr::V4(v4) => {
            matches!(v4.octets(), [10, ..] | [172, 16..=31, ..] | [192, 168, ..])
        }
        _ => false,
    }
}