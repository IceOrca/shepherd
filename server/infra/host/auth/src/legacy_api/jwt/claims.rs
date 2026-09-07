use serde::{Deserialize, Serialize};

use crate::account::Role;

#[cfg(feature = "password-auth")]
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "password-auth", derive(TS))]
pub struct AccessClaims {
    /// Account UUID from the shared accounts table.
    pub sub: String,
    /// Tenant UUID from the shared tenants table.
    pub tid: String,
    pub iss: String,
    pub aud: String,
    pub exp: usize,
    pub nbf: usize,
    pub iat: usize,
    pub jti: String,
    pub sid: String,
    pub username: String,
    pub role: Role,
    pub roles: Vec<String>,
    /// Account authorization version at token issuance.
    pub ver: i64,
    pub permissions: Vec<String>,
}
