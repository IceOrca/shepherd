use std::sync::{Arc, OnceLock};

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    http::HeaderMap,
    response::Response,
    middleware,
    middleware::Next,
    middleware::{from_fn, from_fn_with_state},
    extract::{Extension, Json, State},
};
use std::net::{IpAddr, SocketAddr};
use axum::http::header::{HeaderValue, InvalidHeaderValue};
pub use foundation_kernel::request::OriginatorIp;

use crate::HostContext;
const DEFAULT_TRUSTED_PROXY_CIDRS: &str = "127.0.0.1/32,::1/128";
static TRUSTED_PROXY_CIDRS: OnceLock<Vec<TrustedProxyCidr>> = OnceLock::new();

#[derive(Clone, Debug)]
enum TrustedProxyCidr {
    V4 { network: u32, prefix: u8 },
    V6 { network: u128, prefix: u8 },
}

impl TrustedProxyCidr {
    fn parse(value: &str) -> Option<Self> {
        let (address, prefix) = value.trim().split_once('/')?;
        let prefix: u8 = prefix.parse().ok()?;

        match address.parse::<IpAddr>().ok()? {
            IpAddr::V4(address) if prefix <= 32 => Some(Self::V4 {
                network: u32::from_be_bytes(address.octets()),
                prefix,
            }),
            IpAddr::V6(address) if prefix <= 128 => Some(Self::V6 {
                network: u128::from_be_bytes(address.octets()),
                prefix,
            }),
            _ => None,
        }
    }

    fn contains(&self, address: IpAddr) -> bool {
        match (self, address) {
            (Self::V4 { network, prefix }, IpAddr::V4(address)) => {
                let mask = u32::MAX.checked_shl(u32::from(32 - *prefix)).unwrap_or(0);
                network & mask == u32::from_be_bytes(address.octets()) & mask
            }
            (Self::V6 { network, prefix }, IpAddr::V6(address)) => {
                let mask = u128::MAX.checked_shl(u32::from(128 - *prefix)).unwrap_or(0);
                network & mask == u128::from_be_bytes(address.octets()) & mask
            }
            _ => false,
        }
    }
}

fn trusted_proxy_cidrs() -> &'static [TrustedProxyCidr] {
    TRUSTED_PROXY_CIDRS.get_or_init(|| {
        std::env::var("TRUSTED_PROXY_CIDRS")
            .unwrap_or_else(|_| DEFAULT_TRUSTED_PROXY_CIDRS.to_string())
            .split(',')
            .filter_map(TrustedProxyCidr::parse)
            .collect()
    })
}

pub fn layer(router: Router, state: Arc<HostContext>) -> Router {
    let host_router: Router = router.route_layer(from_fn_with_state(state, originator_ip_layer));
    host_router
}

/// Only trust forwarded headers when they were sent by a configured proxy.
fn extract_originator_ip_trusted(headers: &HeaderMap, socketinfo: Option<&SocketAddr>) -> IpAddr {
    let peer_ip: Option<IpAddr> = socketinfo.map(|addr: &SocketAddr| addr.ip());

    let is_trusted_proxy: bool = peer_ip
        .map(|ip| trusted_proxy_cidrs().iter().any(|cidr| cidr.contains(ip)))
        .unwrap_or(false);

    if is_trusted_proxy {
        // Trust forwarded headers from proxy
        return extract_originator_ip(headers, socketinfo);
    }

    // Not from trusted proxy → use direct IP
    peer_ip.unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

async fn originator_ip_layer(
    State(ctx): State<Arc<HostContext>>,
    ConnectInfo(socketinfo): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Response {
    let ip: IpAddr = extract_originator_ip_trusted(&headers, Some(&socketinfo));
    req.extensions_mut().insert(OriginatorIp::new(ip));
    next.run(req).await
}

/// Extract real client IP behind Caddy/Nginx proxy
pub fn extract_originator_ip(headers: &HeaderMap, socketinfo: Option<&SocketAddr>) -> IpAddr {
    // 1. X-Real-IP — most reliable, single IP set by proxy
    if let Some(ip) = get_x_real_ip(headers) {
        return ip;
    }
    // 2. X-Forwarded-For — take leftmost (original client)
    if let Some(ip) = get_x_forwarded_for(headers) {
        return ip;
    }
    // 3. Forwarded — RFC 7239
    if let Some(ip) = get_forwarded(headers) {
        return ip;
    }

    // 4. Fallback — ConnectInfo (127.0.0.1 behind proxy)
    socketinfo
        .map(|addr: &SocketAddr| addr.ip())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

fn get_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("X-Real-IP")
        .and_then(|v: &HeaderValue| v.to_str().ok())
        .and_then(|s: &str| s.trim().parse::<IpAddr>().ok())
}

fn get_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("X-Forwarded-For")
        .and_then(|v: &HeaderValue| v.to_str().ok())
        .and_then(|s: &str| {
            // "1.2.3.4, proxy1, proxy2" → take first
            s.split(',').next().and_then(|ip| ip.trim().parse::<IpAddr>().ok())
        })
}

fn get_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    // RFC 7239: "Forwarded: for=1.2.3.4;by=proxy"
    headers
        .get("Forwarded")
        .and_then(|v: &HeaderValue| v.to_str().ok())
        .and_then(|s: &str| {
            s.split(';')
                .find(|part: &&str| part.trim().to_lowercase().starts_with("for="))
                .and_then(|part: &str| {
                    part.trim()
                        .trim_start_matches("for=")
                        // Handle IPv6: for="[::1]"
                        .trim_matches('"')
                        .trim_matches('[')
                        .trim_matches(']')
                        .parse::<IpAddr>()
                        .ok()
                })
        })
}

#[cfg(test)]
mod tests {
    use super::TrustedProxyCidr;
    use std::net::IpAddr;

    #[test]
    fn cidr_matches_only_addresses_in_the_same_network() {
        let cidr = TrustedProxyCidr::parse("172.16.0.0/12").expect("valid IPv4 CIDR");

        assert!(cidr.contains("172.31.255.255".parse::<IpAddr>().expect("test address must be valid")));
        assert!(!cidr.contains("172.32.0.1".parse::<IpAddr>().expect("test address must be valid")));
    }
}
