pub mod windows;

use std::hash::Hasher;

/// Hash a string using xxHash64 for use as cache keys
pub fn hash_string(s: &str) -> i64 {
    let mut hasher = twox_hash::XxHash64::default();
    hasher.write(s.to_lowercase().as_bytes());
    hasher.finish() as i64
}
