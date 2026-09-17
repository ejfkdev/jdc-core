//! Process-lifetime caches for debug environment flags.
//!
//! `std::env::var` takes a global lock and walks `environ` on every call; the
//! structuring pipeline checks debug flags at many decision points, and that
//! showed up as a measurable hotspot (`__findenv_locked` at ~2.4% of samples
//! while rendering `rt.jar`). These flags are process-start configuration, so
//! they are frozen on first use — semantics-preserving.

/// Cached presence check for a debug env flag.
#[macro_export]
macro_rules! dbg_flag {
    ($name:literal) => {{
        static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *CACHED.get_or_init(|| std::env::var($name).is_ok())
    }};
}

/// Cached parsed value of a debug env flag.
#[macro_export]
macro_rules! dbg_value {
    ($name:literal, $ty:ty) => {{
        static CACHED: std::sync::OnceLock<Option<$ty>> = std::sync::OnceLock::new();
        CACHED
            .get_or_init(|| {
                std::env::var($name)
                    .ok()
                    .and_then(|v| v.parse::<$ty>().ok())
            })
            .clone()
    }};
}
