
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

/// 供 mDNS 广播的地址列表：只保留**真实局域网网卡**的私有 IPv4。
///
/// 背景：`discover_lan_ips()` 会收集所有私有 IPv4，包含 WSL/Docker/Hyper-V 等
/// 虚拟网卡地址（如 172.17.x.x）——移动端/模拟器 resolve 到这些地址后无法
/// 路由访问，导致"发现成功但连接超时"。此处按适配器名过滤虚拟网卡，
/// 结果为空时回退 `discover_lan_ips()`（保底）。
pub fn mdns_advertise_ips() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        // 虚拟/隧道网卡名特征（小写匹配）。
        const VIRTUAL_MARKERS: &[&str] = &[
            "vehternet", // Hyper-V/WSL 虚拟交换机
            "wsl",
            "docker",
            "hyper-v",
            "virtualbox",
            "vmware",
            "npcap",
            "loopback",
            "bluetooth",
            "isatap",
            "tailscale",
            "zerotier",
            "tun",
            "tap",
        ];

        if let Ok(output) = std::process::Command::new("ipconfig")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut ips: Vec<String> = Vec::new();
            let mut adapter = String::new();
            for raw in stdout.lines() {
                let line = raw.trim();
                // 适配器段头：`<Anything> adapter <Name>:` 或 `<Name> adapter:`
                if let Some(idx) = line.find("adapter") {
                    let head = &line[..idx];
                    let tail = line[idx + "adapter".len()..].trim_end_matches(':').trim();
                    if let Some(name) = tail.split_whitespace().next() {
                        if !name.is_empty() {
                            adapter = name.to_lowercase();
                            continue;
                        }
                    }
                    let _ = head; // 部分系统适配器名在冒号前
                    continue;
                }
                if line.contains("IPv4") && line.contains(':') {
                    if let Some(ip_str) = line.split(':').next_back() {
                        let ip_str = ip_str.trim();
                        if let Ok(addr) = ip_str.parse::<std::net::IpAddr>() {
                            let is_virtual = VIRTUAL_MARKERS
                                .iter()
                                .any(|m| adapter.contains(m));
                            if is_private_ipv4(addr) && !is_virtual && !ips.contains(&ip_str.to_string()) {
                                ips.push(ip_str.to_string());
                            }
                        }
                    }
                }
            }
            if !ips.is_empty() {
                return ips;
            }
        }
    }

    // 非 Windows / 解析失败：回退完整列表（保底，宁多勿漏）。
    discover_lan_ips()
}