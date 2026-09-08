use std::{error::Error, fmt::Write as _, io};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use infra_auth::ext_service::auth_admin::ExtAuthAdmin;
use sqlx::PgPool;
use super::{
    core::{TenantBootstrapRequest, TenantBootstrapOwner, TenantBootstrapResult},
    database,
};
use tracing::{error, warn, info, debug, trace};

pub async fn bootstrap(
    pool: &PgPool,
    auth_admin: &dyn ExtAuthAdmin,
    issuer: &str,
    request: &TenantBootstrapRequest,
    operator: &str,
    email: &str,
) -> Result<TenantBootstrapResult, Box<dyn Error + Send + Sync>> {
    let key: Vec<u8> = provisioning_fingerprint_key()?;
    let fingerprint: String = fingerprint_request(request, &request.owners, &key)?;
    let mut guard: sqlx::Transaction<'static, sqlx::Postgres> = pool.begin().await?;
    let locked: bool = sqlx::query_scalar!(
        r#"SELECT pg_try_advisory_xact_lock(hashtextextended($1, 701)) AS "locked!""#,
        request.idempotency_key.to_string()
    )
    .fetch_one(&mut *guard)
    .await?;
    if !locked {
        return Err(io::Error::other("bootstrap already processing; retry the same request").into());
    }
    let replayed: bool =
        database::claim_bootstrap(pool, request, &fingerprint, operator, email, request.owners.len()).await?;
    if !replayed {
        let result: Result<(), Box<dyn Error + Send + Sync>> = async {
            let owners: Vec<database::ResolvedOwner> =
                database::resolve_owner_identities(pool, auth_admin, request, &request.owners, issuer).await?;
            database::commit_tenant(pool, request, &owners, issuer, operator).await
        }
        .await;
        if let Err(err) = result {
            database::mark_failed(pool, request.idempotency_key, "bootstrap_failed").await;
            return Err(err);
        }
    }
    guard.commit().await?;
    Ok(TenantBootstrapResult {
        tenant_id: request.tenant_id,
        tenant_slug: request.tenant_slug.clone(),
        owner_count: request.owners.len(),
        replayed,
    })
}

fn fingerprint_request(
    args: &TenantBootstrapRequest,
    owners: &[TenantBootstrapOwner],
    key: &[u8],
) -> Result<String, io::Error> {
    let mut digest: Hmac<Sha256> =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| io::Error::other("invalid provisioning fingerprint key"))?;
    update_fingerprint(&mut digest, &args.tenant_id.to_string());
    update_fingerprint(&mut digest, &args.tenant_slug);
    update_fingerprint(&mut digest, &args.tenant_display_name);
    for owner in owners {
        update_fingerprint(&mut digest, &owner.username);
        update_fingerprint(&mut digest, &owner.email);
        update_fingerprint(&mut digest, &owner.password);
    }
    let bytes: [u8; 32] = digest.finalize().into_bytes().into();
    let mut fingerprint: String = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut fingerprint, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(fingerprint)
}

fn update_fingerprint(digest: &mut Hmac<Sha256>, value: &str) {
    digest.update(&value.len().to_be_bytes());
    digest.update(value.as_bytes());
}

fn provisioning_fingerprint_key() -> Result<Vec<u8>, io::Error> {
    let encoded_key: String = std::env::var("AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64")
        .map_err(|_| io::Error::other("provisioning fingerprint key is required"))?;
    let key: Vec<u8> = STANDARD
        .decode(encoded_key)
        .map_err(|_| io::Error::other("AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64 must be valid standard base64"))?;
    if key.len() < 32 {
        return Err(io::Error::other(
            "AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64 must decode to at least 32 bytes",
        ));
    }
    Ok(key)
}
