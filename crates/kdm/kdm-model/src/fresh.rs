//! New replica actors and document ids: random, except in a reproducible build
//! (`SOURCE_DATE_EPOCH` set, as the showcase and gating tests build): there they come from a
//! counter in the process mixed with the epoch, so the handles a new drawing mints (numbered from
//! its actor's slot, [`crate::handle_range`]) and so its saved DXF are the same on every build.
//! Never on the web: the page has no environment.

use std::sync::atomic::{AtomicU64, Ordering};

use kition_kit::kdm::{ActorId, DocId};

/// The ids handed out so far in a reproducible build.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// `SOURCE_DATE_EPOCH` (seconds), when the process has it; read once.
#[cfg(not(target_arch = "wasm32"))]
fn epoch() -> Option<u64> {
    static EPOCH: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(|| std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|value| value.trim().parse::<u64>().ok()))
}

#[cfg(target_arch = "wasm32")]
fn epoch() -> Option<u64> {
    None
}

/// SplitMix64: one well-mixed 64-bit value from a seed.
fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The next counted value, when the build is reproducible.
fn counted() -> Option<u64> {
    let epoch = epoch()?;
    Some(mix(epoch ^ mix(NEXT.fetch_add(1, Ordering::Relaxed))))
}

/// A new actor for a writable replica (the top bit cleared: the engine reserves `u64::MAX`).
pub fn actor() -> ActorId {
    counted().and_then(|value| ActorId::from_u64(value >> 1).ok()).unwrap_or_else(ActorId::random)
}

/// A new document id.
pub fn document() -> DocId {
    let Some(high) = counted() else { return DocId::new() };
    let low = mix(high);
    // A version 4, variant 1 UUID of the two values.
    let high = (high & 0xFFFF_FFFF_FFFF_0FFF) | 0x0000_0000_0000_4000;
    let low = (low & 0x3FFF_FFFF_FFFF_FFFF) | 0x8000_0000_0000_0000;
    let text = format!("{:08x}-{:04x}-{:04x}-{:04x}-{:012x}", high >> 32, (high >> 16) & 0xFFFF, high & 0xFFFF, low >> 48, low & 0xFFFF_FFFF_FFFF);
    DocId::parse(&text).unwrap_or_default()
}
