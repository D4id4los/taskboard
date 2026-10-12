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
            | Self::DetachConnector { v, .. } => Some(*v),
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
    }

    /// The WI-H status field's presence rules: daemon-side under
    /// `nextcloud` carries `connector_attached`, every other projection
    /// omits it (absent, not `null`).
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
            ..attached
        };
        let json = serde_json::to_value(&detached).expect("serialize");
        assert_eq!(json["connector_attached"], false);

        let standalone = StatusView {
            connector_attached: None,
            ..detached
        };
        let json = serde_json::to_value(&standalone).expect("serialize");
        assert!(json.get("connector_attached").is_none());
    }
}
