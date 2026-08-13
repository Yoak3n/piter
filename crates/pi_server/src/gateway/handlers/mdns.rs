//! mDNS 状态 REST 接口（`GET /api/mdns/status`）。
//!
//! 返回当前 gateway 的 mDNS 广播状态（供客户端/调试查看）。

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::gateway::mdns::{MDNS_SERVICE_TYPE, PROTO_VERSION};
use crate::gateway::GatewayState;

/// `GET /api/mdns/status` → `{ enabled, instanceName, port, serviceType, proto }` 或 `{ enabled: false }`。
pub async fn mdns_status_handler(State(state): State<Arc<GatewayState>>) -> Json<Value> {
    let mdns = state.mdns.lock();
    match &*mdns {
        Some(reg) => Json(json!({
            "enabled": true,
            "instanceName": reg.instance_name(),
            "port": reg.port(),
            "serviceType": MDNS_SERVICE_TYPE,
            "proto": PROTO_VERSION,
        })),
        // 未启用时也返回实例名（default_instance_name）——客户端展示用，
        // 即使 mDNS 不可用也能识别服务端身份。
        None => Json(json!({
            "enabled": false,
            "instanceName": crate::gateway::mdns::default_instance_name(),
        })),
    }
}
