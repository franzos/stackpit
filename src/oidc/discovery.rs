//! Late-bound OIDC discovery: retried in the background, published into an [`OidcSlot`].
//! Only the browser surface is late-bindable - `/mcp` mounts at startup and still needs a restart.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use stackpit_auth::BearerGate;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::oidc::client::OidcClient;
use crate::oidc::revocations::SqliteRevocationStore;

const BACKOFF_FLOOR: Duration = Duration::from_secs(1);
const BACKOFF_CEILING: Duration = Duration::from_secs(60);

/// Published as one unit so a request never observes a client without its gate.
pub struct OidcReady {
    pub client: Arc<OidcClient>,
    /// `None` when the discovery doc offered nothing to validate tokens with.
    pub web_gate: Option<BearerGate>,
}

/// Lock-free slot for the discovered surface - read on every authed request.
#[derive(Clone, Default)]
pub struct OidcSlot(Arc<ArcSwapOption<OidcReady>>);

impl OidcSlot {
    #[must_use]
    pub fn ready(ready: OidcReady) -> Self {
        let slot = Self::default();
        slot.publish(ready);
        slot
    }

    pub fn publish(&self, ready: OidcReady) {
        self.0.store(Some(Arc::new(ready)));
    }

    #[must_use]
    pub fn get(&self) -> Option<Arc<OidcReady>> {
        self.0.load_full()
    }

    #[must_use]
    pub fn client(&self) -> Option<Arc<OidcClient>> {
        self.get().map(|r| r.client.clone())
    }

    /// Discovery has landed - not the same as OAuth being configured.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.0.load().is_some()
    }
}

fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(BACKOFF_CEILING)
}

/// Full jitter, so a fleet restarting together doesn't resynchronize on the provider.
fn jittered(backoff: Duration) -> Duration {
    let ms = backoff.as_millis().min(u128::from(u64::MAX)) as u64;
    if ms == 0 {
        return Duration::ZERO;
    }
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).expect("OS RNG must be available");
    Duration::from_millis(u64::from_le_bytes(buf) % (ms + 1))
}

/// Retries until discovery lands, then stops - nothing invalidates a discovered client.
pub fn spawn_retry(
    slot: OidcSlot,
    cancel: CancellationToken,
    config: Arc<Config>,
    revocations: Option<SqliteRevocationStore>,
) {
    crate::background::supervise("oidc_discovery_retry", async move {
        let mut backoff = BACKOFF_FLOOR;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(jittered(backoff)) => {}
            }
            match OidcClient::discover(&config.auth.oauth, config.auth.mcp.jwks_cache_ttl_secs)
                .await
            {
                Ok(client) => {
                    let client = Arc::new(client);
                    let web_gate =
                        crate::server::build_web_bearer_gate(&client, &config, revocations.clone());
                    slot.publish(OidcReady { client, web_gate });
                    tracing::info!(
                        "OIDC discovery succeeded on retry; browser sign-in is live \
                         (/mcp, if configured, still needs a restart)"
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!("OIDC discovery retry failed: {e:#}");
                    backoff = next_backoff(backoff);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_ceiling() {
        assert_eq!(next_backoff(BACKOFF_FLOOR), Duration::from_secs(2));
        assert_eq!(
            next_backoff(Duration::from_secs(16)),
            Duration::from_secs(32)
        );
        assert_eq!(next_backoff(Duration::from_secs(32)), BACKOFF_CEILING);
        assert_eq!(next_backoff(BACKOFF_CEILING), BACKOFF_CEILING);
        assert_eq!(next_backoff(Duration::MAX), BACKOFF_CEILING);
    }

    #[test]
    fn jitter_stays_within_the_backoff() {
        for _ in 0..256 {
            assert!(jittered(Duration::from_secs(4)) <= Duration::from_secs(4));
        }
        assert_eq!(jittered(Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn an_empty_slot_is_not_ready() {
        let slot = OidcSlot::default();
        assert!(!slot.is_ready());
        assert!(slot.client().is_none());
        assert!(slot.get().is_none());
    }
}
