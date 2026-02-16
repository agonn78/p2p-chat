use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub struct BackoffConfig {
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub jitter_ratio: f32,
    pub max_attempts: u32,
}

impl BackoffConfig {
    pub const fn outbox_default() -> Self {
        Self {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            jitter_ratio: 0.2,
            max_attempts: 8,
        }
    }

    pub const fn websocket_default() -> Self {
        Self {
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            jitter_ratio: 0.25,
            max_attempts: u32::MAX,
        }
    }
}

pub fn compute_backoff_delay(config: BackoffConfig, attempts: u32) -> Duration {
    let capped_attempt = attempts.min(31);
    let exponent = 2u32.saturating_pow(capped_attempt);
    let raw_ms = config
        .base_delay
        .as_millis()
        .saturating_mul(exponent as u128)
        .min(config.max_delay.as_millis()) as u64;

    let jitter = jitter_millis(raw_ms, config.jitter_ratio);
    Duration::from_millis(raw_ms.saturating_add(jitter))
}

fn jitter_millis(base_ms: u64, jitter_ratio: f32) -> u64 {
    if base_ms == 0 || jitter_ratio <= 0.0 {
        return 0;
    }

    let max_jitter = (base_ms as f32 * jitter_ratio).round() as u64;
    if max_jitter == 0 {
        return 0;
    }

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    seed % (max_jitter + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_bounded() {
        let config = BackoffConfig {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            jitter_ratio: 0.0,
            max_attempts: 5,
        };

        let first = compute_backoff_delay(config, 0);
        let third = compute_backoff_delay(config, 3);
        let tenth = compute_backoff_delay(config, 10);

        assert_eq!(first, Duration::from_secs(1));
        assert!(third >= Duration::from_secs(5));
        assert_eq!(tenth, Duration::from_secs(5));
    }

    #[test]
    fn jitter_keeps_range() {
        let config = BackoffConfig {
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(2),
            jitter_ratio: 0.5,
            max_attempts: 5,
        };

        let delay = compute_backoff_delay(config, 2);
        assert!(delay >= Duration::from_secs(2));
        assert!(delay <= Duration::from_secs(3));
    }
}
