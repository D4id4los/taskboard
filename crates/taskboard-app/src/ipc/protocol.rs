// SPDX-License-Identifier: AGPL-3.0-only
//! The wire envelopes (ADR 0012 §4.2): one versioned JSON request per
//! connection and one response back. Internally tagged by `"type"`
//! (kebab-case); every versioned variant carries `"v"` so a mismatched
//! peer is detectable *before* any payload misparse. The request carries
//! raw argv words (stable across clap type churn); the response carries
//! rendered text + the process exit code — the client is a dumb relay.

use serde::{Deserialize, Serialize};

/// The wire protocol version. A request whose `v` differs gets a typed
/// [`Response::VersionMismatch`], parseable by any client that knows any
/// version.
pub const PROTOCOL_VERSION: u32 = 1;

/// A client request. `v` is [`PROTOCOL_VERSION`] as sent by the peer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Request {
    /// Run one CLI invocation: argv words *after* the binary name,
    /// exactly as `std::env::args().skip(1)` produced them. The server
    /// is the single parse authority (`Cli::try_parse_from`).
    Exec {
        /// Protocol version.
        v: u32,
        /// The raw argv words.
        argv: Vec<String>,
    },
    /// Live status snapshot (never mutates, never triggers a sync).
    Status {
        /// Protocol version.
        v: u32,
    },
    /// Graceful-shutdown request (the `daemon stop` backend): acked,
    /// then the daemon drains and shuts down.
    Shutdown {
        /// Protocol version.
        v: u32,
    },
    /// Connector-only detach (WI-H): the daemon stops its sync actor and
    /// destroys the in-memory credential, then keeps serving
    /// connector-less. Additive variant (ADR 0012 amendment: no version
    /// bump for additive changes); carries the identity the client just
    /// logged out — the daemon detaches only on an exact match with its
    /// booted identity (decision D3).
    DetachConnector {
        /// Protocol version.
        v: u32,
        /// The server URL of the revoked identity.
        server_url: String,
        /// The username of the revoked identity.
        username: String,
    },
    /// Connector-only attach from persisted state (ADR 0014 decision 7's
    /// daemon-side follow-up): the daemon re-reads its own config and
    /// loads its own credential store — the request carries the identity
    /// only, **never the secret** (the key-set pin proves it). Additive
    /// variant (ADR 0012 amendment: no version bump for additive
    /// changes).
    AttachConnector {
        /// Protocol version.
        v: u32,
        /// The server URL of the identity to attach.
        server_url: String,
        /// The username of the identity to attach.
        username: String,
        /// The target connector's config name (D5-14); `None` = the
        /// default (first) connector. Additive on the wire (no
        /// `PROTOCOL_VERSION` bump — ADR 0012 amendment's additive
        /// precedent): a `None` name serializes to *absent*, so
        /// `None`-carrying requests are byte-identical to the
        /// pre-`name` shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// The server's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    /// Exec outcome: process-shaped exit code plus the exact rendered
    /// streams the in-process dispatch would have printed.
    Exec {
        /// Protocol version.
        v: u32,
        /// Process exit code (0/1/2).
        code: u8,
        /// Rendered stdout bytes.
        out: String,
        /// Rendered stderr bytes.
        err: String,
    },
    /// Live status answer (server side).
    Status {
        /// Protocol version.
        v: u32,
        /// The status view.
        status: crate::commands::status::StatusView,
    },
    /// Shutdown ack — the daemon begins its drain after sending.
    Shutdown {
        /// Protocol version.
        v: u32,
    },
    /// Detach-connector answer (WI-H).
    DetachConnector {
        /// Protocol version.
        v: u32,
        /// What the daemon did (or refused) to do.
        outcome: DetachWireOutcome,
    },
    /// Attach-connector answer (the daemon-side attach from persisted
    /// state).
    AttachConnector {
        /// Protocol version.
        v: u32,
        /// What the daemon did (or refused) to do.
        outcome: AttachWireOutcome,
    },
    /// Any request whose `v` ≠ [`PROTOCOL_VERSION`] gets this, verbatim
    /// and parseable by any client that knows *any* version.
    VersionMismatch {
        /// The server's protocol version.
        server_v: u32,
    },
}

impl Response {
    /// The response's protocol version, when its variant carries one
    /// ([`Response::VersionMismatch`] does not — it must stay parseable
    /// across any version pair).
    #[must_use]
    pub fn version(&self) -> Option<u32> {
        match self {
            Self::Exec { v, .. }
            | Self::Status { v, .. }
            | Self::Shutdown { v }
            | Self::DetachConnector { v, .. }
            | Self::AttachConnector { v, .. } => Some(*v),
            Self::VersionMismatch { .. } => None,
        }
    }
}

/// The detach-connector answer classes (WI-H), kebab-case on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetachWireOutcome {
    /// The sync actor was detached and joined; the credential is gone.
    Detached,
    /// Nothing to detach (standalone daemon, or already detached).
    NoConnector,
    /// The request's identity does not match the daemon's booted one —
    /// no detach (never drop a connector whose credential was not the
    /// one revoked, decision D3).
    IdentityMismatch,
    /// The actor missed the join budget and was aborted; the credential
    /// died with the task either way.
    TimedOut,
}

/// The attach-connector answer classes, kebab-case on the wire — the
/// detach vocabulary mirrored plus the precondition arms the
/// daemon-side construction requires (the secret never rides the wire;
/// the daemon loads it from its own store).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttachWireOutcome {
    /// The daemon built the client from its own persisted state,
    /// attached, and settled its boot pull.
    Attached,
    /// A connector is already attached (the primitive's guard; carried
    /// so the wire reports every primitive refusal honestly).
    AlreadyAttached,
    /// The daemon's re-read config names a different identity than the
    /// request's — nothing changed.
    IdentityMismatch,
    /// The daemon's persisted state could not yield a usable client:
    /// standalone install, missing identity keys, an unreadable or
    /// invalid config, or a URL `DeckClient` rejects. Every arm has the
    /// same remedy — fix the daemon's own config; the daemon logs the
    /// precise cause.
    NotConfigured,
    /// The daemon's credential store could not load the secret (an env
    /// store without `TASKBOARD_APP_PASSWORD` in the daemon's own
    /// environment, or a keyring miss) — the env-store caveat's typed
    /// surface.
    CredentialUnavailable,
    /// The state engine could not take the attach push.
    Engine,
    /// The state engine declined the attach push (a raced shutdown).
    Draining,
}

#[cfg(test)]
mod tests {
    use super::{PROTOCOL_VERSION, Request, Response};
    use crate::commands::status::StatusView;

    /// The frozen v1 wire shapes: the `"type"` tags are kebab-case and
    /// every versioned variant serializes its `v`. A shape drift here is
    /// a protocol break, not a refactor.
    #[test]
    fn wire_shapes_are_pinned() {
        let exec = Request::Exec {
            v: PROTOCOL_VERSION,
            argv: vec!["tasks".to_owned(), "list".to_owned()],
        };
        let json = serde_json::to_value(&exec).expect("serialize");
        assert_eq!(json["type"], "exec");
        assert_eq!(json["v"], 1);
        assert_eq!(json["argv"][0], "tasks");

        let status = Request::Status {
            v: PROTOCOL_VERSION,
        };
        assert_eq!(
            serde_json::to_value(&status).expect("serialize")["type"],
            "status"
        );

        let shutdown = Request::Shutdown {
            v: PROTOCOL_VERSION,
        };
        assert_eq!(
            serde_json::to_value(&shutdown).expect("serialize")["type"],
            "shutdown"
        );

        let detach = Request::DetachConnector {
            v: PROTOCOL_VERSION,
            server_url: "https://cloud.example.com".to_owned(),
            username: "alice".to_owned(),
        };
        let json = serde_json::to_value(&detach).expect("serialize");
        assert_eq!(json["type"], "detach-connector");
        assert_eq!(json["v"], 1);
        assert_eq!(json["server_url"], "https://cloud.example.com");
        assert_eq!(json["username"], "alice");

        let attach = Request::AttachConnector {
            v: PROTOCOL_VERSION,
            server_url: "https://cloud.example.com".to_owned(),
            username: "alice".to_owned(),
            name: None,
        };
        let json = serde_json::to_value(&attach).expect("serialize");
        assert_eq!(json["type"], "attach-connector");
        assert_eq!(json["v"], 1);
        assert_eq!(json["server_url"], "https://cloud.example.com");
        assert_eq!(json["username"], "alice");

        let response = Response::Exec {
            v: 1,
            code: 0,
            out: "out".to_owned(),
            err: String::new(),
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["type"], "exec");
        assert_eq!(json["code"], 0);

        let mismatch = Response::VersionMismatch { server_v: 2 };
        let json = serde_json::to_value(&mismatch).expect("serialize");
        assert_eq!(json["type"], "version-mismatch");
        assert_eq!(json["server_v"], 2);

        let view = StatusView {
            daemon: true,
            version: Some("0.1.0".to_owned()),
            backend: Some("standalone".to_owned()),
            pid: Some(42),
            sync: None,
            holder_pid: None,
            connector_attached: None,
        };
        let json = serde_json::to_value(Response::Status { v: 1, status: view }).expect("s");
        assert_eq!(json["type"], "status");
        assert_eq!(json["status"]["daemon"], true);
        assert_eq!(json["status"]["pid"], 42);

        let answer = Response::DetachConnector {
            v: 1,
            outcome: crate::ipc::protocol::DetachWireOutcome::IdentityMismatch,
        };
        let json = serde_json::to_value(&answer).expect("serialize");
        assert_eq!(json["type"], "detach-connector");
        assert_eq!(json["outcome"], "identity-mismatch");

        let answer = Response::AttachConnector {
            v: 1,
            outcome: crate::ipc::protocol::AttachWireOutcome::CredentialUnavailable,
        };
        let json = serde_json::to_value(&answer).expect("serialize");
        assert_eq!(json["type"], "attach-connector");
        assert_eq!(json["outcome"], "credential-unavailable");
    }

    /// The refused-secret invariant, structural: the attach payload's
    /// serialized key sets are exactly the identity vocabulary — there
    /// is no field a secret could ride. Any future field addition fails
    /// this pin and forces a conscious decision.
    #[test]
    fn the_attach_request_carries_identity_only() {
        let request = Request::AttachConnector {
            v: PROTOCOL_VERSION,
            server_url: "https://cloud.example.com".to_owned(),
            username: "alice".to_owned(),
            name: None,
        };
        let json = serde_json::to_value(&request).expect("serialize");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("request object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["server_url", "type", "username", "v"]);

        // The named target (D5-14): the key rides the wire only when
        // present; the absent form decodes as `None` (additive).
        let named = Request::AttachConnector {
            v: PROTOCOL_VERSION,
            server_url: "https://cloud.example.com".to_owned(),
            username: "alice".to_owned(),
            name: Some("beta".to_owned()),
        };
        let json = serde_json::to_value(&named).expect("serialize");
        assert_eq!(json["name"], "beta");
        let back: Request =
            serde_json::from_value(serde_json::json!({"type": "attach-connector", "v": 1,
                "server_url": "https://cloud.example.com", "username": "alice"}))
            .expect("the absent name decodes");
        assert_eq!(back, request);

        let response = Response::AttachConnector {
            v: PROTOCOL_VERSION,
            outcome: crate::ipc::protocol::AttachWireOutcome::Attached,
        };
        let json = serde_json::to_value(&response).expect("serialize");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("response object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["outcome", "type", "v"]);
    }

    /// Round-trips through serde are lossless for every variant.
    #[test]
    fn envelopes_round_trip() {
        let requests = [
            Request::Exec {
                v: 1,
                argv: vec!["--json".to_owned()],
            },
            Request::Status { v: 1 },
            Request::Shutdown { v: 1 },
            Request::DetachConnector {
                v: 1,
                server_url: "https://c.example.com".to_owned(),
                username: "bob".to_owned(),
            },
            Request::AttachConnector {
                v: 1,
                server_url: "https://c.example.com".to_owned(),
                username: "bob".to_owned(),
                name: None,
            },
            Request::AttachConnector {
                v: 1,
                server_url: "https://c.example.com".to_owned(),
                username: "bob".to_owned(),
                name: Some("beta".to_owned()),
            },
        ];
        for request in requests {
            let bytes = serde_json::to_vec(&request).expect("serialize");
            let back: Request = serde_json::from_slice(&bytes).expect("deserialize");
            assert_eq!(back, request);
        }
        let responses = [
            Response::Exec {
                v: 1,
                code: 2,
                out: String::new(),
                err: "usage".to_owned(),
            },
            Response::Status {
                v: 1,
                status: StatusView {
                    daemon: false,
                    version: None,
                    backend: None,
                    pid: None,
                    sync: None,
                    holder_pid: Some(7),
                    connector_attached: None,
                },
            },
            Response::Shutdown { v: 1 },
            Response::DetachConnector {
                v: 1,
                outcome: super::DetachWireOutcome::Detached,
            },
            Response::AttachConnector {
                v: 1,
                outcome: super::AttachWireOutcome::Attached,
            },
            Response::VersionMismatch { server_v: 9 },
        ];
        for response in responses {
            let bytes = serde_json::to_vec(&response).expect("serialize");
            let back: Response = serde_json::from_slice(&bytes).expect("deserialize");
            assert_eq!(back, response);
        }
    }

    /// Optional fields are absent (not `null`) when `None` — the status
    /// payload stays minimal and the discriminating `daemon` bool plus
    /// present-fields decide the shape.
    #[test]
    fn absent_optional_fields_are_skipped() {
        let view = StatusView {
            daemon: false,
            version: None,
            backend: None,
            pid: None,
            sync: None,
            holder_pid: None,
            connector_attached: None,
        };
        let json = serde_json::to_value(&view).expect("serialize");
        assert_eq!(json, serde_json::json!({ "daemon": false }));
    }

    /// The response version accessor drives the client's mismatch
    /// detection; `VersionMismatch` is deliberately version-less.
    #[test]
    fn version_accessor_skips_only_version_mismatch() {
        assert_eq!(Response::VersionMismatch { server_v: 3 }.version(), None);
        assert_eq!(Response::Shutdown { v: 1 }.version(), Some(1));
        assert_eq!(
            Response::DetachConnector {
                v: 1,
                outcome: super::DetachWireOutcome::NoConnector,
            }
            .version(),
            Some(1)
        );
        assert_eq!(
            Response::AttachConnector {
                v: 1,
                outcome: super::AttachWireOutcome::NotConfigured,
            }
            .version(),
            Some(1)
        );
    }

    /// The WI-H status field's presence rules: every daemon-side answer
    /// carries the live `connector_attached` (both backends — a
    /// standalone-booted daemon can attach from persisted state);
    /// client-side and in-session projections omit it unless the caller
    /// gated the fact in (absent, not `null`).
    #[test]
    fn connector_attached_presence_is_pinned() {
        let attached = StatusView {
            daemon: true,
            version: Some("0.1.0".to_owned()),
            backend: Some("nextcloud".to_owned()),
            pid: Some(1),
            sync: None,
            holder_pid: None,
            connector_attached: Some(true),
        };
        let json = serde_json::to_value(&attached).expect("serialize");
        assert_eq!(json["connector_attached"], true);

        let detached = StatusView {
            connector_attached: Some(false),
            ..attached.clone()
        };
        let json = serde_json::to_value(&detached).expect("serialize");
        assert_eq!(json["connector_attached"], false);

        // Daemon-side standalone: booted connector-less pins `false`
        // (post-attach is the `Some(true)` row above, backend aside).
        let standalone_daemon = StatusView {
            backend: Some("standalone".to_owned()),
            ..detached.clone()
        };
        let json = serde_json::to_value(&standalone_daemon).expect("serialize");
        assert_eq!(json["connector_attached"], false);

        // Client-side / in-session without a fact: absent.
        let standalone = StatusView {
            connector_attached: None,
            ..detached
        };
        let json = serde_json::to_value(&standalone).expect("serialize");
        assert!(json.get("connector_attached").is_none());
    }
}
