// SPDX-License-Identifier: MIT OR Apache-2.0
//! The scripted in-process `WebDAV` server behind the `test-support`
//! feature (decision D-FT12): the fake-transport matrix's medium.
//!
//! The fake is a *model*, not a mock library: it stores files and
//! collection revisions (etags churn per PUT — F5's upload-churn rule
//! is the model's behavior, so nothing can accidentally lean on an
//! etag as content identity), records every request verbatim (the
//! exact observable: method, path, conditional headers, body), and
//! takes one-shot *scenario directives* (garbage/torn bodies, verbatim
//! listings, 412-once answers, vanishing subtrees) that the actor
//! tests stage between cycles.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

use crate::error::TransportError;
use crate::etag::Etag;
use crate::transport::{
    FileContent, FilesTransport, MkcolOutcome, PropfindListing, PropfindMember, PutOutcome,
};

/// One recorded request (the tests' exact observable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    /// The raw `If-Match` header value when sent (verbatim).
    pub if_match: Option<String>,
    /// Whether `If-None-Match: *` was sent (create-once).
    pub if_none_match_star: bool,
    pub body: Vec<u8>,
}

/// A one-shot scenario directive, consumed FIFO before the fake
/// answers any request.
#[derive(Debug, Clone)]
pub enum Effect {
    /// The next request is answered normally (an alignment marker so
    /// a failure injection can target a specific request in the
    /// sequence).
    Pass,
    /// The next request is answered with this raw status (no model
    /// change) — the failure-injection arm.
    Status(u16),
    /// The next GET/PROPFIND success is answered with this body —
    /// the garbage/torn-body arm.
    Body(Vec<u8>),
    /// The next PROPFIND is answered with this listing verbatim —
    /// the reorder/duplicate arm.
    Listing(Vec<PropfindMember>),
}

struct FakeState {
    /// Root-inclusive path → (bytes, revision). The revision is the
    /// etag's only ingredient (churns on every PUT, F5).
    files: BTreeMap<String, (Vec<u8>, u64)>,
    /// Collection path → revision; a child write bumps every ancestor
    /// (the F6 dirty-marker fact).
    dirs: BTreeMap<String, u64>,
    requests: Vec<RecordedRequest>,
    effects: VecDeque<Effect>,
    /// Paths whose next `put_if_match` answers 412 with no model
    /// change (the unexpected-precondition arm; the file's fate after
    /// it — kept or removed — is what the fork/vanish scenarios set).
    four12_once: BTreeSet<String>,
}

/// The scripted server. Cheap to clone for assertions is not needed —
/// share it in an `Arc` between the test and the spawned actor.
pub struct FakeFilesTransport {
    state: Mutex<FakeState>,
}

impl Default for FakeFilesTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeFilesTransport {
    /// An empty account. Attach scenarios call [`Self::seed_dirs`] so
    /// the MKCOL ladder rides the exists-ok posture; first-attach
    /// scenarios leave it bare.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(FakeState {
                files: BTreeMap::new(),
                dirs: BTreeMap::new(),
                requests: Vec::new(),
                effects: VecDeque::new(),
                four12_once: BTreeSet::new(),
            }),
        }
    }

    /// Creates the given collections (201 on re-create is 405 — this
    /// seeds the exists-ok path).
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn seed_dirs(&self, dirs: &[&str]) {
        let mut state = self.state.lock().expect("fake lock");
        for dir in dirs {
            state.dirs.insert((*dir).to_owned(), 0);
        }
    }

    /// Writes a file into the model as if a peer (or a former install)
    /// had PUT it, bumping ancestor collection revisions.
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn seed_file(&self, path: &str, bytes: Vec<u8>) {
        let mut state = self.state.lock().expect("fake lock");
        state.files.insert(path.to_owned(), (bytes, 1));
        bump_ancestors(&mut state.dirs, path);
    }

    /// Removes a file (the vanish/delete scenario arm).
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn remove_file(&self, path: &str) {
        let mut state = self.state.lock().expect("fake lock");
        state.files.remove(path);
        bump_ancestors(&mut state.dirs, path);
    }

    /// Removes a device's whole subtree (the vanished-device arm) —
    /// `taskboard/devices/<id>` and everything under it.
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn remove_device_subtree(&self, devices_prefix: &str) {
        let mut state = self.state.lock().expect("fake lock");
        state
            .files
            .retain(|path, _| !path.starts_with(devices_prefix));
        state
            .dirs
            .retain(|path, _| !path.starts_with(devices_prefix));
    }

    /// Stages a one-shot directive.
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn stage(&self, effect: Effect) {
        self.state
            .lock()
            .expect("fake lock")
            .effects
            .push_back(effect);
    }

    /// The next `put_if_match` on `path` answers 412, model unchanged.
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn answer_412_once(&self, path: &str) {
        self.state
            .lock()
            .expect("fake lock")
            .four12_once
            .insert(path.to_owned());
    }

    /// The request log so far.
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.state.lock().expect("fake lock").requests.clone()
    }

    /// Count of logged requests matching a method (prefix-matched
    /// path when given).
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn count(&self, method: &str, path_prefix: Option<&str>) -> usize {
        self.requests()
            .iter()
            .filter(|req| {
                req.method == method
                    && path_prefix.is_none_or(|prefix| req.path.starts_with(prefix))
            })
            .count()
    }

    /// The current bytes of a model file.
    ///
    /// # Panics
    ///
    /// Only if the model mutex is poisoned by a panicking test — never by
    /// design (no lock is held across any await).
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.state
            .lock()
            .expect("fake lock")
            .files
            .get(path)
            .map(|(bytes, _)| bytes.clone())
    }

    fn record(state: &mut FakeState, req: RecordedRequest) {
        state.requests.push(req);
    }

    fn take_effect(state: &mut FakeState) -> Option<Effect> {
        state.effects.pop_front()
    }
}

fn etag_of(rev: u64) -> Etag {
    Etag::new(format!("\"rev{rev}\""))
}

fn bump_ancestors(dirs: &mut BTreeMap<String, u64>, path: &str) {
    let mut ancestors = path.split('/').collect::<Vec<_>>();
    ancestors.pop(); // the file itself is not a collection
    let mut prefix = String::new();
    for component in ancestors {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        dirs.entry(prefix.clone()).and_modify(|rev| *rev += 1);
    }
}

fn listing_members(state: &FakeState, prefix: &str) -> Vec<PropfindMember> {
    // Direct child collections of `prefix`, in the model's order.
    let prefix = format!("{prefix}/");
    state
        .dirs
        .iter()
        .filter(|(path, _)| {
            path.strip_prefix(&prefix)
                .is_some_and(|rest| rest.split('/').count() == 1 && !rest.is_empty())
        })
        .map(|(path, rev)| PropfindMember {
            href: format!("{path}/"),
            etag: Some(etag_of(*rev)),
        })
        .collect()
}

impl FilesTransport for FakeFilesTransport {
    #[allow(clippy::unused_async_trait_impl)] // the model is synchronous
    async fn mkcol(&self, path: &str) -> Result<MkcolOutcome, TransportError> {
        let mut state = self.state.lock().expect("fake lock");
        Self::record(
            &mut state,
            RecordedRequest {
                method: "MKCOL".into(),
                path: path.into(),
                if_match: None,
                if_none_match_star: false,
                body: Vec::new(),
            },
        );
        if let Some(Effect::Status(status)) = Self::take_effect(&mut state) {
            return Err(status_error(status));
        }
        if state.dirs.contains_key(path) {
            return Ok(MkcolOutcome::AlreadyExists);
        }
        // A collection needs its parent (the 409-on-missing-parent
        // posture the wiremock tier pins against the real server).
        let parent = path.rsplit_once('/').map(|(p, _)| p);
        if parent.is_some_and(|p| !state.dirs.contains_key(p)) {
            return Err(TransportError::Conflict);
        }
        state.dirs.insert(path.to_owned(), 0);
        Ok(MkcolOutcome::Created)
    }

    #[allow(clippy::unused_async_trait_impl)] // the model is synchronous
    async fn propfind_devices(&self) -> Result<PropfindListing, TransportError> {
        let mut state = self.state.lock().expect("fake lock");
        Self::record(
            &mut state,
            RecordedRequest {
                method: "PROPFIND".into(),
                path: "taskboard/devices/".into(),
                if_match: None,
                if_none_match_star: false,
                body: Vec::new(),
            },
        );
        match Self::take_effect(&mut state) {
            None | Some(Effect::Pass) => {} // propfind falls through to the model listing
            Some(Effect::Status(status)) => return Err(status_error(status)),
            Some(Effect::Body(body)) => {
                return crate::extract::extract_207(&body).map_err(TransportError::from);
            }
            Some(Effect::Listing(members)) => {
                return Ok(PropfindListing { members });
            }
        }
        Ok(PropfindListing {
            members: listing_members(&state, "taskboard/devices"),
        })
    }

    #[allow(clippy::unused_async_trait_impl)] // the model is synchronous
    async fn get(&self, path: &str) -> Result<FileContent, TransportError> {
        let mut state = self.state.lock().expect("fake lock");
        Self::record(
            &mut state,
            RecordedRequest {
                method: "GET".into(),
                path: path.into(),
                if_match: None,
                if_none_match_star: false,
                body: Vec::new(),
            },
        );
        match Self::take_effect(&mut state) {
            None | Some(Effect::Pass | Effect::Listing(_)) => {}
            Some(Effect::Status(status)) => return Err(status_error(status)),
            Some(Effect::Body(body)) => {
                let rev = state.files.get(path).map_or(0, |(_, rev)| *rev);
                return Ok(FileContent {
                    bytes: body,
                    etag: Some(etag_of(rev)),
                });
            }
        }
        match state.files.get(path) {
            Some((bytes, rev)) => Ok(FileContent {
                bytes: bytes.clone(),
                etag: Some(etag_of(*rev)),
            }),
            None => Err(TransportError::NotFound),
        }
    }

    #[allow(clippy::unused_async_trait_impl)] // the model is synchronous
    async fn put_create(&self, path: &str, body: &[u8]) -> Result<PutOutcome, TransportError> {
        let mut state = self.state.lock().expect("fake lock");
        Self::record(
            &mut state,
            RecordedRequest {
                method: "PUT".into(),
                path: path.into(),
                if_match: None,
                if_none_match_star: true,
                body: body.to_vec(),
            },
        );
        if let Some(Effect::Status(status)) = Self::take_effect(&mut state) {
            return Err(status_error(status));
        }
        if state.files.contains_key(path) {
            // `If-None-Match: *` against an existing file: 412 (F3).
            return Err(TransportError::PreconditionFailed);
        }
        let rev = state
            .files
            .insert(path.to_owned(), (body.to_vec(), 1))
            .map_or(1, |_| 1);
        bump_ancestors(&mut state.dirs, path);
        Ok(PutOutcome::Created {
            etag: Some(etag_of(rev)),
        })
    }

    #[allow(clippy::unused_async_trait_impl)] // the model is synchronous
    async fn put_if_match(
        &self,
        path: &str,
        body: &[u8],
        etag: &Etag,
    ) -> Result<PutOutcome, TransportError> {
        let mut state = self.state.lock().expect("fake lock");
        Self::record(
            &mut state,
            RecordedRequest {
                method: "PUT".into(),
                path: path.into(),
                if_match: Some(etag.to_string()),
                if_none_match_star: false,
                body: body.to_vec(),
            },
        );
        if let Some(Effect::Status(status)) = Self::take_effect(&mut state) {
            return Err(status_error(status));
        }
        if state.four12_once.remove(path) {
            // The unexpected precondition: no model change (the fork /
            // vanish scenarios decide the file's fate around it).
            return Err(TransportError::PreconditionFailed);
        }
        let Some((_, served_rev)) = state.files.get_mut(path) else {
            return Err(TransportError::NotFound);
        };
        if etag_of(*served_rev).to_string() != etag.to_string() {
            return Err(TransportError::PreconditionFailed);
        }
        *served_rev += 1;
        let rev = *served_rev;
        state.files.get_mut(path).unwrap().0 = body.to_vec();
        bump_ancestors(&mut state.dirs, path);
        Ok(PutOutcome::Updated {
            etag: Some(etag_of(rev)),
        })
    }
}

/// Raw-status injection mapped through the Deck status table (the fake
/// never invents error *kinds* the real server cannot produce).
fn status_error(status: u16) -> TransportError {
    match status {
        404 => TransportError::NotFound,
        409 => TransportError::Conflict,
        412 => TransportError::PreconditionFailed,
        429 => TransportError::RateLimited { retry_after: None },
        503 => TransportError::Unavailable,
        other => TransportError::Server(other),
    }
}
