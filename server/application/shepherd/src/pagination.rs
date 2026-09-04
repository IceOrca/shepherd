use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Serialize, de::DeserializeOwned};

use crate::ListPaginationCfg;

pub fn resolve_limit(config: &ListPaginationCfg, requested: Option<u16>) -> Result<u16, StatusCode> {
    let limit: u16 = requested.unwrap_or(config.def_limit);
    if !(config.min_limit..=config.max_limit).contains(&limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(limit)
}

pub fn decode_cursor<T: DeserializeOwned>(value: Option<&str>) -> Result<Option<T>, StatusCode> {
    value
        .map(|encoded: &str| {
            let bytes: Vec<u8> = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| StatusCode::BAD_REQUEST)?;
            serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)
        })
        .transpose()
}

pub fn encode_cursor<T: Serialize>(cursor: Option<&T>) -> Result<Option<String>, StatusCode> {
    cursor
        .map(|value: &T| {
            let bytes: Vec<u8> = serde_json::to_vec(value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(URL_SAFE_NO_PAD.encode(bytes))
        })
        .transpose()
}

pub fn normalize_search(search: Option<String>) -> Option<String> {
    search.and_then(|value: String| {
        let normalized: String = value.trim().to_lowercase();
        if normalized.is_empty() { None } else { Some(normalized) }
    })
}
