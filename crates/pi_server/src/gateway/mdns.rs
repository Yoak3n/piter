//! mDNS 服务发现注册（`_piter._tcp` 广播）。
//!
//! 对齐 mock-contract §4：服务类型 `_piter._tcp`，TXT 记录 `port`/`proto`/`name`。
//! 使用 [mdns-sd]（纯 Rust、跨平台；Windows 10+ 原生支持 mDNS 组播）。
//! 注册失败不致命——mDNS 只是便利层，扫码/手动输入是保底通路。
//!
//! [mdns-sd]: https://crates.io/crates/mdns-sd

use std::collections::HashMap;

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// mDNS 服务类型（`_piter._tcp.local.`，契约 §4）。
pub const MDNS_SERVICE_TYPE: &str = "_piter._tcp.local.";

/// TXT key：实际端口。
pub const TXT_PORT: &str = "port";
/// TXT key：协议版本（能力探测用）。
pub const TXT_PROTO: &str = "proto";
/// TXT key：实例名（可读标识，客户端列表显示）。
pub const TXT_NAME: &str = "name";

/// 协议版本值（基线 "1"）。
pub const PROTO_VERSION: &str = "1";

/// 已注册的 mDNS 服务句柄；`stop()` 注销并关闭 daemon。
pub struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullname: String,
    instance_name: String,
    port: u16,
}

impl MdnsRegistration {
    /// 注册 `_piter._tcp` 广播。失败返回 Err（mDNS 不可用不阻塞 gateway）。
    pub fn start(port: u16, instance_name: &str) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| format!("[mdns] daemon: {e}"))?;

        let mut props = HashMap::new();
        props.insert(TXT_PORT.to_string(), port.to_string());
        props.insert(TXT_PROTO.to_string(), PROTO_VERSION.to_string());
        props.insert(TXT_NAME.to_string(), instance_name.to_string());

        // host_name 必须以 .local. 结尾（mdns-sd 校验）。
        let host_name = format!("{}.local.", host_basename());

        // 只广播真实局域网网卡的地址——否则 addr_auto 会把 WSL/Docker/Hyper-V
        // 等虚拟网卡 IP（如 172.17.x.x）一起广播，客户端 resolve 到后无法连接。
        let advertise = crate::gateway::helper::mdns_advertise_ips();
        let ip_refs: Vec<&str> = advertise.iter().map(|s| s.as_str()).collect();

        let info = ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            instance_name,
            &host_name,
            &ip_refs[..],
            port,
            props,
        )
        .map_err(|e| format!("[mdns] service_info: {e}"))?;

        let fullname = info.get_fullname().to_string();
        daemon
            .register(info)
            .map_err(|e| format!("[mdns] register: {e}"))?;

        Ok(Self {
            daemon,
            fullname,
            instance_name: instance_name.to_string(),
            port,
        })
    }

    /// 注销服务并关闭 daemon（进程退出时 OS 会自动释放组播 socket）。
    pub fn stop(&self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }

    pub fn fullname(&self) -> &str {
        &self.fullname
    }

    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

/// 本机主机名（兜底 "piter"）。
fn host_basename() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "piter".to_string())
}

/// 默认实例名：环境变量 `PITER_MDNS_NAME` 优先，否则取主机名（兜底 "Piter"）。
pub fn default_instance_name() -> String {
    if let Ok(name) = std::env::var("PITER_MDNS_NAME") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    host_basename()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_instance_name_non_empty() {
        let name = default_instance_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn env_override_wins() {
        unsafe { std::env::set_var("PITER_MDNS_NAME", "书房 Piter") };
        assert_eq!(default_instance_name(), "书房 Piter");
    }

    #[test]
    fn service_info_contains_expected_txt() {
        let mut props = HashMap::new();
        props.insert(TXT_PORT.to_string(), "31421".to_string());
        props.insert(TXT_PROTO.to_string(), PROTO_VERSION.to_string());
        props.insert(TXT_NAME.to_string(), "test-piter".to_string());

        let info = ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            "test-piter",
            "",
            (), // 地址：空集 + enable_addr_auto
            31421,
            props,
        )
        .unwrap()
        .enable_addr_auto();

        assert!(info.get_fullname().starts_with("test-piter."));
        assert!(info.get_type().starts_with("_piter._tcp"));
        assert_eq!(info.get_port(), 31421);
        assert_eq!(info.get_property_val_str(TXT_PORT), Some("31421"));
        assert_eq!(info.get_property_val_str(TXT_PROTO), Some("1"));
        assert_eq!(info.get_property_val_str(TXT_NAME), Some("test-piter"));
    }

    #[test]
    #[ignore = "需要真实网络组播；手动运行：cargo test -p pi_server mdns -- --ignored --nocapture"]
    fn browse_discovers_own_service() {
        let reg = MdnsRegistration::start(31421, "integration-piter").expect("register");
        let daemon = ServiceDaemon::new().expect("daemon");
        let receiver = daemon.browse(MDNS_SERVICE_TYPE).expect("browse");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut found = false;
        while std::time::Instant::now() < deadline {
            if let Ok(evt) = receiver.recv_timeout(std::time::Duration::from_millis(200)) {
                if let mdns_sd::ServiceEvent::ServiceResolved(info) = evt {
                    if info.get_fullname().contains("integration-piter") {
                        found = true;
                        break;
                    }
                }
            }
        }
        reg.stop();
        assert!(found, "未通过 mDNS 发现自身服务");
    }
}
