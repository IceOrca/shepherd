#![cfg_attr(debug_assertions, allow(unused))]

pub mod database;

pub use database::{DatabaseAdapter, PostgresCli, TenantDbErr, TenantTransaction};
