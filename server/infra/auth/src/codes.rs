use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeserializeError};
#[cfg(feature = "ext-service")]
use ts_rs::TS;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "ext-service", derive(TS))]
pub struct RoleCode(String);

impl RoleCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, AuthCodeError> {
        let value: String = value.into();
        if is_valid_role_code(&value) {
            Ok(Self(value))
        } else {
            Err(AuthCodeError::InvalidRoleCode)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for RoleCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for RoleCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RoleCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: String = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

impl TryFrom<String> for RoleCode {
    type Error = AuthCodeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for RoleCode {
    type Error = AuthCodeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value.to_owned())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "ext-service", derive(TS))]
pub struct PermissionCode(String);

impl PermissionCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, AuthCodeError> {
        let value: String = value.into();
        if is_valid_permission_code(&value) {
            Ok(Self(value))
        } else {
            Err(AuthCodeError::InvalidPermissionCode)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for PermissionCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for PermissionCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PermissionCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: String = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

impl TryFrom<String> for PermissionCode {
    type Error = AuthCodeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for PermissionCode {
    type Error = AuthCodeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value.to_owned())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthCodeError {
    InvalidRoleCode,
    InvalidPermissionCode,
}

impl fmt::Display for AuthCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoleCode => formatter.write_str("role code format is invalid"),
            Self::InvalidPermissionCode => formatter.write_str("permission code format is invalid"),
        }
    }
}

impl Error for AuthCodeError {}

fn is_valid_role_code(value: &str) -> bool {
    let mut characters: std::str::Chars<'_> = value.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };
    value.len() <= 255
        && first_character.is_ascii_lowercase()
        && characters.clone().next().is_some()
        && characters.all(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_valid_permission_code(value: &str) -> bool {
    if value.len() > 255 {
        return false;
    }
    let segments: Vec<&str> = value.split('.').collect();
    segments.len() >= 2 && segments.into_iter().all(is_valid_permission_segment)
}

fn is_valid_permission_segment(segment: &str) -> bool {
    let mut characters: std::str::Chars<'_> = segment.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };
    first_character.is_ascii_lowercase()
        && characters.all(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{PermissionCode, RoleCode};

    #[test]
    fn role_codes_follow_database_catalog_format() {
        assert!(RoleCode::parse("owner").is_ok());
        assert!(RoleCode::parse("employee2").is_ok());
        assert!(RoleCode::parse("x").is_err());
        assert!(RoleCode::parse("TenantOwner").is_err());
        assert!(RoleCode::parse("tenant-owner").is_err());
    }

    #[test]
    fn permission_codes_require_normalized_segments() {
        assert!(PermissionCode::parse("records.items.read").is_ok());
        assert!(PermissionCode::parse("auth.accounts.disable").is_ok());
        assert!(PermissionCode::parse("accounts").is_err());
        assert!(PermissionCode::parse("auth..read").is_err());
        assert!(PermissionCode::parse("Auth.accounts.read").is_err());
    }

    #[test]
    fn authorization_codes_serialize_as_wire_strings() -> Result<(), serde_json::Error> {
        let role: RoleCode = serde_json::from_str(r#""custom_role""#)?;
        let permission: PermissionCode = serde_json::from_str(r#""auth.accounts.read""#)?;

        assert_eq!(role.as_str(), "custom_role");
        assert_eq!(permission.as_str(), "auth.accounts.read");
        assert_eq!(serde_json::to_string(&role)?, r#""custom_role""#);
        assert_eq!(serde_json::to_string(&permission)?, r#""auth.accounts.read""#);
        Ok(())
    }
}
