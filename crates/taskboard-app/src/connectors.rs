// SPDX-License-Identifier: AGPL-3.0-only
//! The connector module seam (design §3.4): one configured connector
//! implementation builds its client from config + secrets and spawns its
//! sync actor over the shared vocabulary (`SyncCommand` / `SyncReport` /
//! `SyncStateReader`, ADR 0007/0010). The narrow seam: no discovery, no
//! dynamic loading, no plugin registry, no versioning — config names a
//! module, the app constructs it.
//!
//! Resolution (D5-2): `sync.connectors = Some(entries)` means exactly the
//! array, in file order, the first entry the default connector
//! (`Some(vec![])` is the array door to standalone); the array absent
//! means the legacy scalar world verbatim — a resolved `nextcloud`
//! synthesizes one connector named `nextcloud` from the `[nextcloud]`
//! table, a resolved `standalone` synthesizes none. The loop over
//! [`configured_connectors`] is the N-actor seam.

use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use taskboard_domain::{ConnectorId, SyncCommand, SyncReport, SyncStateReader, SystemEvent};
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};

use crate::bootstrap::AttachError;
use crate::config::{AppConfig, ConfigError, SyncBackend};
use crate::secrets::CredentialStore;

/// The name the legacy scalar world synthesizes (DF-3's mint-chain head).
/// Also the inert default under `standalone` — no bindings exist to
/// decode there, so the constant is never consumed.
pub(crate) const DEFAULT_CONNECTOR_NAME: &str = "nextcloud";

/// A running connector's identity plus its join handle. The command
/// channel's *sender* never crosses this boundary (D-6): the bootstrap
/// owns the command pair — it is created before the engine spawn, whose
/// nudge registry needs the sender — so the module spawns the receiver it
/// was handed and the caller keeps the pair-mate sender as the registry
/// entry's inbox.
pub struct SpawnedConnector {
    /// The config-minted connector id (the registry key).
    pub id: ConnectorId,
    /// The actor's join handle (bounded joins are the caller's policy).
    pub join: JoinHandle<()>,
}

/// One configured connector implementation: builds its client from
/// config + secrets and spawns its actor over the shared vocabulary.
/// Object-safe by construction; bootstrap `dyn`-dispatches the registry
/// loop over the configured set.
pub trait ConnectorModule: Send + Sync {
    /// The connector's config-minted id.
    fn id(&self) -> &ConnectorId;

    /// Builds the connector's client and spawns its actor on the current
    /// runtime over the bootstrap-owned channels.
    ///
    /// # Errors
    ///
    /// [`AttachError::Credential`] when the app password cannot be
    /// resolved, [`AttachError::Client`] when the Deck client rejects the
    /// server URL. The channels are the bootstrap's; a failed build leaks
    /// nothing but the dropped receiver.
    fn build_and_spawn(
        &self,
        commands: mpsc::Receiver<SyncCommand>,
        engine_reader: Arc<dyn SyncStateReader>,
        reports: mpsc::Sender<SyncReport>,
        system: broadcast::Sender<SystemEvent>,
        cfg: SyncActorConfig,
    ) -> Result<SpawnedConnector, AttachError>;
}

/// The Nextcloud connector: one config slice + credential store.
pub struct NextcloudModule {
    id: ConnectorId,
    server_url: String,
    username: String,
    credentials: Arc<dyn CredentialStore>,
}

impl NextcloudModule {
    #[must_use]
    pub fn new(
        id: ConnectorId,
        server_url: String,
        username: String,
        credentials: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            id,
            server_url,
            username,
            credentials,
        }
    }
}

impl ConnectorModule for NextcloudModule {
    fn id(&self) -> &ConnectorId {
        &self.id
    }

    fn build_and_spawn(
        &self,
        commands: mpsc::Receiver<SyncCommand>,
        engine_reader: Arc<dyn SyncStateReader>,
        reports: mpsc::Sender<SyncReport>,
        system: broadcast::Sender<SystemEvent>,
        cfg: SyncActorConfig,
    ) -> Result<SpawnedConnector, AttachError> {
        let password = self
            .credentials
            .load(&self.server_url, &self.username)
            .map_err(AttachError::Credential)?;
        // F12-03: one cleartext warn per process — bootstrap runs once, so
        // every front end that learns the URL from config is warned exactly
        // once (the warn moved here with client construction, D-14).
        if crate::config::url_is_cleartext(&self.server_url) {
            tracing::warn!(
                server = %self.server_url,
                "server_url uses cleartext http; the app password \
                 travels unencrypted — prefer https"
            );
        }
        let client = DeckClient::new(&self.server_url, &self.username, password.expose())
            .map_err(AttachError::Client)?;
        let join = spawn_sync_actor(
            self.id.clone(),
            client,
            engine_reader,
            commands,
            reports,
            system,
            cfg,
        );
        Ok(SpawnedConnector {
            id: self.id.clone(),
            join,
        })
    }
}

/// One entry of the configured connector set: the config-minted name,
/// the module selector, and the identity the module builds its client
/// from (per-connector — never the legacy single-connector accessors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorSpec {
    /// The config-minted connector name.
    pub name: ConnectorId,
    /// The connector kind (the module selector).
    pub backend: SyncBackend,
    /// The connector's server URL.
    pub server_url: String,
    /// The connector's login name.
    pub username: String,
}

/// The configured connector set — the single source of truth for "how
/// many connectors exist" (D5-2):
///
/// - `sync.connectors = Some(entries)` ⇒ exactly the array, in file
///   order; the first entry is the default connector. `Some(vec![])` is
///   a valid zero-connector world (the array door to standalone).
/// - `sync.connectors = None` ⇒ the legacy world verbatim:
///   `sync_backend()`'s auto rule stands; a resolved `nextcloud`
///   synthesizes exactly one connector named `nextcloud` with the
///   `[nextcloud]` identity; a resolved `standalone` synthesizes none.
#[allow(clippy::result_large_err)] // mirrors the enum's one grant above
pub(crate) fn configured_connectors(config: &AppConfig) -> Result<Vec<ConnectorSpec>, ConfigError> {
    if let Some(entries) = &config.sync.connectors {
        return entries
            .iter()
            .map(|entry| {
                ConnectorId::from_kebab(&entry.name)
                    .map(|name| ConnectorSpec {
                        name,
                        backend: entry.backend,
                        server_url: entry.server_url.clone(),
                        username: entry.username.clone(),
                    })
                    .map_err(|_| ConfigError::InvalidConnectorName {
                        name: entry.name.clone(),
                    })
            })
            .collect();
    }
    match config.sync_backend() {
        SyncBackend::Nextcloud => Ok(vec![ConnectorSpec {
            // The const is `"nextcloud"`; proven kebab by the pinned
            // unit test below, so the mint cannot fail by construction.
            name: ConnectorId::from_kebab(DEFAULT_CONNECTOR_NAME)
                .expect("DEFAULT_CONNECTOR_NAME is kebab"),
            backend: SyncBackend::Nextcloud,
            server_url: config.server_url().to_owned(),
            username: config.username().to_owned(),
        }]),
        SyncBackend::Standalone => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectorCfg;

    fn config_with_backend(backend: SyncBackend) -> AppConfig {
        let mut config = AppConfig::default();
        config.sync.backend = Some(backend);
        config.nextcloud.server_url = Some("https://cloud.example.com".into());
        config.nextcloud.username = Some("alice".into());
        config
    }

    #[test]
    fn resolved_nextcloud_synthesizes_exactly_one_connector_named_nextcloud() {
        let specs = configured_connectors(&config_with_backend(SyncBackend::Nextcloud)).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name.as_str(), "nextcloud");
        assert_eq!(specs[0].backend, SyncBackend::Nextcloud);
    }

    #[test]
    fn auto_resolved_nextcloud_synthesizes_the_connector() {
        // server_url set, backend unset: the auto rule resolves nextcloud.
        let mut config = config_with_backend(SyncBackend::Nextcloud);
        config.sync.backend = None;
        let specs = configured_connectors(&config).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name.as_str(), "nextcloud");
    }

    #[test]
    fn resolved_standalone_synthesizes_none() {
        let specs = configured_connectors(&config_with_backend(SyncBackend::Standalone)).unwrap();
        assert_eq!(specs, Vec::new(), "standalone synthesizes no connector");
    }

    fn config_with_array(entries: &[(&str, &str, &str, &str)]) -> AppConfig {
        let mut config = AppConfig::default();
        config.sync.connectors = Some(
            entries
                .iter()
                .map(|(name, backend, url, user)| ConnectorCfg {
                    name: (*name).into(),
                    backend: if *backend == "standalone" {
                        SyncBackend::Standalone
                    } else {
                        SyncBackend::Nextcloud
                    },
                    server_url: (*url).into(),
                    username: (*user).into(),
                })
                .collect(),
        );
        config
    }

    #[test]
    fn array_world_specs_are_exactly_the_array_in_file_order() {
        let config = config_with_array(&[
            ("alpha", "nextcloud", "https://a.example", "ua"),
            ("beta", "nextcloud", "https://b.example", "ub"),
        ]);
        let specs = configured_connectors(&config).unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].name.as_str(), "alpha", "first entry = default");
        assert_eq!(specs[0].server_url, "https://a.example");
        assert_eq!(specs[0].username, "ua");
        assert_eq!(specs[1].name.as_str(), "beta");
        assert_eq!(specs[1].server_url, "https://b.example");
    }

    #[test]
    fn empty_array_world_resolves_zero_connectors() {
        let mut config = config_with_array(&[]);
        config.storage.db_path = Some(std::path::PathBuf::from("/tmp/t.db"));
        config.validate().expect("valid world");
        assert_eq!(configured_connectors(&config).unwrap(), Vec::new());
    }

    #[test]
    fn the_default_connector_name_is_kebab() {
        // Pins the `expect` in `configured_connectors`'s legacy-synthesis
        // arm: the const mint cannot fail by construction.
        assert!(ConnectorId::is_kebab_name(DEFAULT_CONNECTOR_NAME));
    }
}
