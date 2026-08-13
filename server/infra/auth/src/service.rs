use std::sync::Arc;

use crate::keycloak::{KeycloakAuth, KeycloakAuthError};

/// Authentication capability exposed by the HTTP host.
///
/// Keycloak owns credentials and browser sessions. The service intentionally
/// stays small: it gives host/application middleware access to verified OIDC
/// identities without exposing the optional legacy authentication stack.
pub struct AuthService {
    pub keycloak: Arc<KeycloakAuth>,
}

impl AuthService {
    pub async fn from_env() -> Result<Arc<Self>, KeycloakAuthError> {
        Ok(Arc::new(Self {
            keycloak: KeycloakAuth::from_env().await?,
        }))
    }
}
