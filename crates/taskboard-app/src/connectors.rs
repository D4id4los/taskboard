// SPDX-License-Identifier: AGPL-3.0-only
//! The connector module seam (design §3.4): one configured connector
//! implementation builds its client from config + secrets and spawns its
//! sync actor over the shared vocabulary (`SyncCommand` / `SyncReport` /
//! `SyncStateReader`, ADR 0007/0010). The narrow seam: no discovery, no
//! dynamic loading, no plugin registry, no versioning — config names a
//! module, the app constructs it.
//!
//! This chunk keeps the legacy scalar config world (design §3.6's
//! synthesis-of-one subset): a resolved `nextcloud` backend synthesizes
//! exactly one connector named `nextcloud`; a resolved `standalone`
//! synthesizes none. The `[[sync.connectors]]` plural decode is chunk 5's;
//! the loop over [`configured_connectors`] *is* the seam it needs.

use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use taskboard_domain::{ConnectorId, SyncCommand, SyncReport, SyncStateReader, SystemEvent};
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};

use crate::bootstrap::AttachError;
use crate::config::{AppConfig, SyncBackend};
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

/// One entry of the configured connector set (the synthesis-of-one
/// subset; chunk 5 replaces the internals with `[[sync.connectors]]`
/// decode, the shape stays).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectorSpec {
    /// The config-minted connector name.
    pub name: ConnectorId,
    /// The connector kind (the module selector).
    pub backend: SyncBackend,
}

/// The configured connector set from the legacy scalar world: resolved
/// `nextcloud` synthesizes exactly one connector named `nextcloud`;
/// resolved `standalone` synthesizes none. Config reading stays entirely
/// legacy (`sync_backend()`'s auto rule, the `[nextcloud]` table) — no
/// array decode exists in this chunk.
pub(crate) fn configured_connectors(config: &AppConfig) -> Vec<ConnectorSpec> {
    match config.sync_backend() {
        SyncBackend::Nextcloud => vec![ConnectorSpec {
            name: ConnectorId::from(DEFAULT_CONNECTOR_NAME),
            backend: SyncBackend::Nextcloud,
        }],
        SyncBackend::Standalone => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_backend(backend: SyncBackend) -> AppConfig {
        let mut config = AppConfig::default();
        config.sync.backend = Some(backend);
        config.nextcloud.server_url = Some("https://cloud.example.com".into());
        config.nextcloud.username = Some("alice".into());
        config
    }

    #[test]
    fn resolved_nextcloud_synthesizes_exactly_one_connector_named_nextcloud() {
        let specs = configured_connectors(&config_with_backend(SyncBackend::Nextcloud));
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name.as_str(), "nextcloud");
        assert_eq!(specs[0].backend, SyncBackend::Nextcloud);
    }

    #[test]
    fn auto_resolved_nextcloud_synthesizes_the_connector() {
        // server_url set, backend unset: the auto rule resolves nextcloud.
        let mut config = config_with_backend(SyncBackend::Nextcloud);
        config.sync.backend = None;
        let specs = configured_connectors(&config);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name.as_str(), "nextcloud");
    }

    #[test]
    fn resolved_standalone_synthesizes_none() {
        let specs = configured_connectors(&config_with_backend(SyncBackend::Standalone));
        assert_eq!(specs, Vec::new(), "standalone synthesizes no connector");
    }
}
