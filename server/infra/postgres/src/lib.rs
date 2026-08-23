#![cfg_attr(debug_assertions, allow(unused))]

pub mod database;

pub use database::{DatabaseAdapter, PostgresCli, TenantDbErr, TenantTransaction, active_branch_id, with_active_branch};
