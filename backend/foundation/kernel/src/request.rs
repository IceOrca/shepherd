use std::net::IpAddr;

/// Client IP resolved by the host after applying its trusted-proxy policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginatorIp(IpAddr);

impl OriginatorIp {
    pub fn new(ip: IpAddr) -> Self {
        Self(ip)
    }

    pub fn ip(&self) -> IpAddr {
        self.0
    }
}

/// Stable identity key inserted by an authentication feature for rate limiting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalRateLimitKey(String);

impl PrincipalRateLimitKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
