use std::{error::Error, io};
use infra_auth::ext_service::auth_admin::ExternalIdentity;
use infra_postgres::DatabaseAdapter;
use supabase_auth::SupabaseAuthAdmin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let username: String = std::env::var("SYSTEM_ADMIN_USERNAME")?;
    let email: String = std::env::var("SYSTEM_ADMIN_EMAIL")?.trim().to_lowercase();
    let password: String = std::env::var("SYSTEM_ADMIN_PASSWORD")?;
    let issuer: String = std::env::var("AUTH_ISSUER_URL")?;
    if username.trim().is_empty() || !email.contains('@') || password.len() < 8 {
        return Err(io::Error::other("Invalid system administrator configuration").into());
    }
    let adapter: std::sync::Arc<SupabaseAuthAdmin> = SupabaseAuthAdmin::from_env()?;
    let identity: ExternalIdentity = adapter
        .provision_platform_identity(&username, &email, &password)
        .await?;
    let database: std::sync::Arc<DatabaseAdapter> = DatabaseAdapter::new_arc().await;
    let mut transaction: sqlx::Transaction<'static, sqlx::Postgres> = database.pool().begin().await?;
    sqlx::query!("INSERT INTO platform_administrators (issuer, subject, username, email) VALUES ($1, $2, $3, $4) ON CONFLICT (issuer, subject) DO UPDATE SET username = EXCLUDED.username, email = EXCLUDED.email, is_active = TRUE", issuer, identity.subject, username, email)
        .execute(&mut *transaction).await?;
    sqlx::query!("INSERT INTO platform_administration_events (actor_issuer, actor_subject, action, details) VALUES ($1, $2, 'administrator.initialize', '{}'::jsonb)", issuer, identity.subject)
        .execute(&mut *transaction).await?;
    transaction.commit().await?;
    println!("System administrator initialized; credentials remain managed by Supabase Auth.");
    Ok(())
}
