use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::header;
use axum::{middleware::Next, response::Response};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::{Level, Span, info_span};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => {
        tracing::error!($($arg)+)
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => {
        tracing::warn!($($arg)+)
    };
}

#[macro_export]
macro_rules! notice {
    ($($arg:tt)+) => {
        tracing::info!($($arg)+)
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => {
        tracing::info!($($arg)+)
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => {
        tracing::debug!($($arg)+)
    };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)+) => {
        tracing::trace!($($arg)+)
    };
}

pub use crate::error;

pub use crate::warn;

pub use crate::notice;

pub use crate::info;

pub use crate::debug;

pub use crate::trace;

pub struct Category(pub u64);

impl Category {
    pub const NONE: u64 = 0;
    pub const SYSTEM: u64 = 1 << 0; // 0x0001
    pub const AUTH: u64 = 1 << 1; // 0x0002
    pub const DATABASE: u64 = 1 << 2; // 0x0004
    pub const HTTP: u64 = 1 << 3; // 0x0008
    pub const BILLING: u64 = 1 << 4; // 0x0010
    pub const CACHE: u64 = 1 << 5; // 0x0020
    pub const QUEUE: u64 = 1 << 6; // 0x0040
    pub const SECURITY: u64 = 1 << 7; // 0x0080
    // ...
    pub const ALL: u64 = u64::MAX;

    pub fn name(cat: u64) -> &'static str {
        match cat {
            Self::SYSTEM => "System",
            Self::AUTH => "Auth",
            Self::DATABASE => "Database",
            Self::HTTP => "Http",
            Self::BILLING => "Billing",
            Self::CACHE => "Cache",
            Self::QUEUE => "Queue",
            Self::SECURITY => "Security",
            Self::ALL => "All",
            _ => "Unknown",
        }
    }
}

pub struct SubCat(pub u64);

impl SubCat {
    pub const NONE: u64 = 0;

    // AUTH subcategories
    pub const LOGIN: u64 = 1 << 0;
    pub const LOGOUT: u64 = 1 << 1;
    pub const REGISTER: u64 = 1 << 2;
    pub const TOKEN: u64 = 1 << 3;
    pub const PERMISSION: u64 = 1 << 4;

    // DATABASE subcategories
    pub const QUERY: u64 = 1 << 0;
    pub const MIGRATION: u64 = 1 << 1;
    pub const CONNECTION: u64 = 1 << 2;
    pub const POOL: u64 = 1 << 3;
    pub const TXMIT: u64 = 1 << 4; // transaction

    // HTTP subcategories
    pub const REQUEST: u64 = 1 << 0;
    pub const RESPONSE: u64 = 1 << 1;
    pub const MIDDLEWARE: u64 = 1 << 2;
    pub const WEBSOCKET: u64 = 1 << 3;

    // SYSTEM subcategories
    pub const STARTUP: u64 = 1 << 0;
    pub const SHUTDOWN: u64 = 1 << 1;
    pub const CONFIG: u64 = 1 << 2;
    pub const HEALTH: u64 = 1 << 3;

    // ...

    pub const ALL: u64 = u64::MAX;

    pub fn name(sub: u64) -> &'static str {
        match sub {
            Self::NONE => "None",
            Self::LOGIN => "Login",
            Self::LOGOUT => "Logout",
            Self::REGISTER => "Register",
            Self::TOKEN => "Token",
            Self::PERMISSION => "Permission",
            Self::QUERY => "Query",
            Self::MIGRATION => "Migration",
            Self::CONNECTION => "Connection",
            Self::POOL => "Pool",
            Self::TXMIT => "Transaction",
            Self::REQUEST => "Request",
            Self::RESPONSE => "Response",
            Self::MIDDLEWARE => "Middleware",
            Self::WEBSOCKET => "Websocket",
            Self::STARTUP => "Startup",
            Self::SHUTDOWN => "Shutdown",
            Self::CONFIG => "Config",
            Self::HEALTH => "Health",
            Self::ALL => "All",
            // ...
            _ => "Unknown",
        }
    }
}

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

pub struct LogLevel(pub u8);

impl LogLevel {
    pub const ALWAYS: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const WARN: u8 = 2;
    pub const INFO: u8 = 3;
    pub const NOTICE: u8 = 4;
    pub const DEBUG: u8 = 5;
    pub const TRACE: u8 = 6;
}

static ENABLED_CATEGORIES: AtomicU64 = AtomicU64::new(u64::MAX);
static ENABLED_SUBS: AtomicU64 = AtomicU64::new(u64::MAX);
static ENABLED_LEVEL: AtomicU8 = AtomicU8::new(u8::MAX);

pub struct LogCtrl;

impl LogCtrl {
    pub fn set(categories: u64, subs: u64, level: u8) {
        ENABLED_CATEGORIES.store(categories, Ordering::Relaxed);
        ENABLED_SUBS.store(subs, Ordering::Relaxed);
        ENABLED_LEVEL.store(level, Ordering::Relaxed);
    }

    pub fn set_categories(categories: u64) {
        ENABLED_CATEGORIES.store(categories, Ordering::Relaxed);
    }

    pub fn set_subcategories(subs: u64) {
        ENABLED_SUBS.store(subs, Ordering::Relaxed);
    }

    pub fn set_level(level: u8) {
        ENABLED_LEVEL.store(level, Ordering::Relaxed);
    }

    pub fn enable_category(cat: u64) {
        ENABLED_CATEGORIES.fetch_or(cat, Ordering::Relaxed);
    }

    pub fn disable_category(cat: u64) {
        ENABLED_CATEGORIES.fetch_and(!cat, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn is_enabled(cat: u64, sub: u64, level: u8) -> bool {
        if level <= LogLevel::ERROR {
            return true;
        }
        let cats: u64 = ENABLED_CATEGORIES.load(Ordering::Relaxed);
        let subs: u64 = ENABLED_SUBS.load(Ordering::Relaxed);
        let loglevel: u8 = ENABLED_LEVEL.load(Ordering::Relaxed);

        (cats & cat != 0) && (subs & sub != 0) && (loglevel <= level)
    }
}

#[macro_export]
macro_rules! log_always {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     tracing::error!("[ALWAYS] [{cat}][{sub}] {msg}",
    //         cat = $crate::debug::Category::name($cat),
    //         sub = $crate::debug::SubCat::name($sub),
    //         msg = format_args!($($arg)+),
    //     )
    // };

    ($($arg:tt)+) => {
        tracing::error!("[ALWAYS] {}", format_args!($($arg)+));
    };
}

#[macro_export]
macro_rules! log_error {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     if $crate::debug::LogCtrl::is_enabled($cat, $sub,
    //         $crate::debug::LogLevel::ERROR
    //     ) {
    //         tracing::error!("[{cat}][{sub}] {msg}",
    //             cat = $crate::debug::Category::name($cat),
    //             sub = $crate::debug::SubCat::name($sub),
    //             msg = format_args!($($arg)+),
    //         )
    //     }
    // };

    ($($arg:tt)+) => {
        if $crate::debug::LogCtrl::is_enabled(
            $crate::debug::Category::ALL,
            $crate::debug::SubCat::ALL,
            $crate::debug::LogLevel::ERROR
        ) {
            tracing::error!("{}", format_args!($($arg)+));
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     if $crate::debug::LogCtrl::is_enabled($cat, $sub,
    //         $crate::debug::LogLevel::WARN
    //     ) {
    //         tracing::warn!("[{cat}][{sub}] {msg}",
    //             cat = $crate::debug::Category::name($cat),
    //             sub = $crate::debug::SubCat::name($sub),
    //             msg = format_args!($($arg)+),
    //         )
    //     }
    // };

    ($($arg:tt)+) => {
        if $crate::debug::LogCtrl::is_enabled(
            $crate::debug::Category::ALL,
            $crate::debug::SubCat::ALL,
            $crate::debug::LogLevel::WARN
        ) {
            tracing::warn!("{}", format_args!($($arg)+));
        }
    };
}

#[macro_export]
macro_rules! log_notice {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     if $crate::debug::LogCtrl::is_enabled($cat, $sub,
    //         $crate::debug::LogLevel::NOTICE
    //     ) {
    //         tracing::info!("[{cat}][{sub}] {msg}",
    //             cat = $crate::debug::Category::name($cat),
    //             sub = $crate::debug::SubCat::name($sub),
    //             msg = format_args!($($arg)+),
    //         )
    //     }
    // };

    ($($arg:tt)+) => {
        if $crate::debug::LogCtrl::is_enabled(
            $crate::debug::Category::ALL,
            $crate::debug::SubCat::ALL,
            $crate::debug::LogLevel::NOTICE
        ) {
            tracing::info!("{}", format_args!($($arg)+));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     if $crate::debug::LogCtrl::is_enabled($cat, $sub,
    //         $crate::debug::LogLevel::INFO
    //     ) {
    //         tracing::info!("[{cat}][{sub}] {msg}",
    //             cat = $crate::debug::Category::name($cat),
    //             sub = $crate::debug::SubCat::name($sub),
    //             msg = format_args!($($arg)+),
    //         )
    //     }
    // };

    ($($arg:tt)+) => {
        if $crate::debug::LogCtrl::is_enabled(
            $crate::debug::Category::ALL,
            $crate::debug::SubCat::ALL,
            $crate::debug::LogLevel::INFO
        ) {
            tracing::info!("{}", format_args!($($arg)+));
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     if $crate::debug::LogCtrl::is_enabled($cat, $sub,
    //         $crate::debug::LogLevel::DEBUG
    //     ) {
    //         tracing::debug!("[{cat}][{sub}] {msg}",
    //             cat = $crate::debug::Category::name($cat),
    //             sub = $crate::debug::SubCat::name($sub),
    //             msg = format_args!($($arg)+),
    //         )
    //     }
    // };

    ($($arg:tt)+) => {
        if $crate::debug::LogCtrl::is_enabled(
            $crate::debug::Category::ALL,
            $crate::debug::SubCat::ALL,
            $crate::debug::LogLevel::DEBUG
        ) {
            tracing::debug!("{}", format_args!($($arg)+));
        }
    };
}

#[macro_export]
macro_rules! log_trace {
    // ($cat:expr, $sub:expr, $($arg:tt)+) => {
    //     if $crate::debug::LogCtrl::is_enabled($cat, $sub,
    //         $crate::debug::LogLevel::TRACE
    //     ) {
    //         tracing::trace!("[{cat}][{sub}] {msg}",
    //             cat = $crate::debug::Category::name($cat),
    //             sub = $crate::debug::SubCat::name($sub),
    //             msg = format_args!($($arg)+),
    //         )
    //     }
    // };

    ($($arg:tt)+) => {
        if $crate::debug::LogCtrl::is_enabled(
            $crate::debug::Category::ALL,
            $crate::debug::SubCat::ALL,
            $crate::debug::LogLevel::TRACE
        ) {
            tracing::trace!("{}", format_args!($($arg)+));
        }
    };
}

pub use crate::log_always;

pub use crate::log_error;

pub use crate::log_warn;

pub use crate::log_notice;

pub use crate::log_info;

pub use crate::log_debug;

pub use crate::log_trace;

pub struct Debugging;

impl Debugging {
    pub fn init() {
        LogCtrl::set(Category::ALL, SubCat::ALL, LogLevel::INFO);

        let tracing_registry: Registry = tracing_subscriber::registry();
        tracing_registry
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                println!("RUST_LOG environment variable not set, defaulting to 'info' level");
                LogCtrl::set_level(LogLevel::INFO);
                "info".into()
            }))
            .with(
                fmt::layer()
                    .with_level(true) // log level
                    .with_target(true) // module name
                    .with_file(false) // file name
                    .with_line_number(true) // line number
                    .with_thread_ids(true) // thread IDs (multithreading)
                    .pretty(), // Format with colors and newlines for better readability in console
            )
            .init();

        tracing::info!("Tracing System initialized");
    }
}
