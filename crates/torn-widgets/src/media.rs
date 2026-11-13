//! Time arithmetic shared by media widgets.

use std::time::Duration;

/// Returns `value` reduced modulo `modulus`, or `value` when `modulus` is zero.
pub(crate) fn duration_remainder(value: Duration, modulus: Duration) -> Duration {
    if modulus.is_zero() {
        return value;
    }
    let nanos = value.as_nanos() % modulus.as_nanos();
    let seconds = u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX);
    let subsecond = u32::try_from(nanos % 1_000_000_000).unwrap_or(0);
    Duration::new(seconds, subsecond)
}
