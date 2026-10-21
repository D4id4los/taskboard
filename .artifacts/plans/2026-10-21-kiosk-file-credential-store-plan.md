# [Plan] Kiosk Chunk 12 — Encrypted-File Credential Store

- **Date**: 2026-10-21
- **Kind**: stage-1 TDD-first implementation plan (dispatch plan §12,
  prompt 12.1) for chunk 12 of the kiosk implementation round.
- **Branch / worktree**: `feat/app-file-credential-store` at
  `../taskboard-WT-feat-app-file-credential-store`, created off
  `origin/main` = `47398b3` (local `main` stood 84 commits behind
  `origin/main` — the round's chunks 1–10 have been merging — so the
  dispatch's "then-current `main`" is `origin/main`; every seam below
  was re-located against that commit, MD-8).
- **Normative sources** (MD-7 rank): implementation plan §2 KDF-9 /
  §3 K12 / §4 / §5 / §6; ADR 0008 (the `CredentialStore` port
  decision — this chunk is the amendment to its "stays backlogged"
  clause); dispatch plan §1/§12 incl. MD-10's dependency gate;
  briefing §1.8 (D-K8's option (a) and its threat-model honesty
  paragraph); issues #159/#126. **The design gate is CLOSED**: D-K8
  ruled option (a) on 2026-10-19 (generated 0600 key file + AEAD
  envelope; passphrase/argon2 out of scope). Nothing in this plan
  deviates from an ACCEPTED ruling; the one judgement call inside the
  ruling's grant — which of the two pre-named AEAD crates — is
  decision D1 below, with evidence.

## 1. Scope

**In** (implementation plan §3 K12; KDF-9): the third
`CredentialStore` implementation — `FileCredentialStore` — an AEAD
envelope at rest under a generated 0600 key file; the
`[nextcloud] credential_store = "file"` selector arm; the key-file
bootstrap ceremony (generate on first use, 0600 enforced with a typed
violation error); typed failure fences for every corruption shape;
the two workspace dependency pins (`chacha20poly1305`, `getrandom`)
behind MD-10's gate; ADR 0008 amendment; README credential-store
documentation; CHANGELOG line; tests per the S-series conventions.

**Out** (non-goals, §3 K12): no passphrase/argon2 variant (ruled out
of scope; a future additive port impl if a desktop need appears —
then with stack approval); no key rotation; no multi-profile keyring;
no wire change (secrets never ride IPC — already pinned by test); no
`CredentialStore` port signature change (unmoved by ruling); no
merging of the two pre-existing factory duplicates (both gain the
arm, the dedup is not this chunk's); no new config keys beyond the
enum arm.

## 2. Verified baseline (re-located 2026-10-21 @ `47398b3`)

Symbols are the stable keys; line numbers are plan-day references.

| Seam | Location @ `47398b3` | Fact |
|---|---|---|
| The port | `crates/taskboard-app/src/secrets.rs:72` | `CredentialStore` trait: `load/store/delete(&self, server, username)` — **unmoved** |
| `AppPassword` | `secrets.rs:29` | redacting `Debug`, `expose()` only; leak-floor pinned by S1 |
| `CredentialError` | `secrets.rs:53` | `NotFound` / `Backend(keyring::Error)` / `Unsupported` |
| `KeyringStore` | `secrets.rs:109` | default; `NoEntry`→`NotFound` mapping at `secrets.rs:149` |
| `EnvCredentialStore` | `secrets.rs:162` | read-only escape hatch; `Unsupported` on writes |
| `credential_key` | `secrets.rs:194` | `{username}@{host}`, stable for garbage inputs |
| Factory 1 | `secrets.rs:211` | `resolve_credential_store(kind)` — the booting lifecycles' wiring point |
| Factory 2 | `commands/mod.rs:469` | `production_store(kind)` — the CLI commands' wiring point |
| Call sites | `main.rs:317`, `ui.rs:282`, `repl.rs:786`, `commands/mod.rs:1085` (connect), `:1128` (logout), `:1724` (`core_ctx`) | all six hold the loaded `AppConfig` in scope |
| Config kind | `config.rs:129` | `CredentialStoreKind { Keyring, Env }`, kebab-case, `#[default] Keyring` |
| Config field | `config.rs:150` | `NextcloudSection.credential_store`; **process-global, not per-connector** (the ruled comment at `config.rs:487` — also valid inside the `[[sync.connectors]]` array world) |
| Decode test | `config.rs:1252` | the kebab decode pattern to mirror for `"file"` |
| Display arm | `commands/config.rs:75` | the `config` verb's kind→string match gains an arm |
| db path | `config.rs:823` `db_path()`, `:931` `default_db_path()` | resolved at load; always `Some` after `extract_raw`/`load` |
| S-series tests | `tests/secrets.rs` S1–S7 | redaction, key shape, env store, keyring mapping, fake round-trip, `#[ignore]`d real-keyring |
| Fixtures | `tests/common/mod.rs` | `tempdir()`, `env_lock()`, `FakeCredentialStore` |

**Dependency facts** (verified 2026-10-21 via `Cargo.lock`, `cargo
tree -i`, `cargo info`):

- No AEAD crate is in the tree. `chacha20 0.10.2` **is already in
  `Cargo.lock`** (proptest's RNG, dev-graph only). `aes 0.8.4` is in
  the **release** graph via `keyring → dbus-secret-service` — the old
  major; `aes-gcm 0.11` pairs with `aes 0.9`, so adopting it
  duplicates the aes major (`multiple-versions` warn).
- `getrandom 0.4.3` is already in the **release** graph (`uuid
  1.28.0`, `tempfile 3.27.0`, `quinn-proto`, `rand 0.10.3`, plus
  dev-graph `jobserver`); 0.2.17 and 0.3.4 also exist (multiple
  majors already coexist, `warn` only).
- `chacha20poly1305 0.11.0`: Apache-2.0 OR MIT, `rust-version` 1.85
  (workspace MSRV **1.96** clears), features `default = [alloc,
  getrandom]`, optional `zeroize`. `aead 0.6.1`: MIT OR Apache-2.0,
  same rust-version. `poly1305`/`universal-hash`/`aead` are absent
  from the lock — the complete new-to-lock set is those three plus
  `chacha20poly1305` itself.
- `deny.toml`: global allow covers MIT and Apache-2.0; `multiple-
  versions = "warn"`; sources locked to crates.io.
- The wave's own deployment lane (K9 runbook, the D250 finding)
  includes an **i686 target whose CPU class has no AES-NI**.

## 3. Decisions

Each decision is final for stage 2 unless marked otherwise;
deviations from ACCEPTED rulings would go back to the user (none do).

### D1 — AEAD crate: `chacha20poly1305 0.11` (within the D-K8/MD-10 grant)

`chacha20poly1305 = { version = "0.11", default-features = false,
features = ["alloc", "zeroize"] }`. The `getrandom` feature stays
**off** — randomness flows through the direct `getrandom` pin (D2),
keeping `rand_core` out of the wiring.

Evidence, in deciding order:

1. **Grant**: D-K8's option (a) as recorded in briefing §1.8 names
   exactly two crates — "`aes-gcm` or `chacha20poly1305`: pure Rust,
   no system deps, deny-friendly" — and MD-10 repeats the pair.
   Choosing between the two named crates *is* the grant; nothing
   here needs a STOP-and-ask.
2. **Hardware uniformity on this project's own targets**: ChaCha20-
   Poly1305 is constant-time ARX on every CPU with zero
   hardware-feature dependence. AES-GCM's fast path needs AES-NI +
   PCLMULQDQ; on the wave's own i686 D250 lane (no AES-NI) it falls
   back to soft AES + soft GHASH — correct (the RustCrypto soft
   implementations are constant-time) but slower and dragging the
   `cpufeatures` detection surface along. The payload is one
   password-sized blob per login/boot, so raw performance is a
   non-issue either way — which leaves hardware-uniformity as a
   deciding axis, and it is ChaCha's.
3. **Tree cost** (verified §2): ChaCha20-Poly1305 reuses `chacha20
   0.10.2` already in the lock; the AES-GCM set (`aes 0.9`, `gcm`,
   `ghash`, `polyval`) is entirely new **and** duplicates the
   in-release-graph `aes 0.8.4`. New-to-lock crates: 4 vs 6+.
4. **No interop requirement**: the envelope is private to taskboard
   (no age/CLI interop, no protocol compatibility), which removes
   AES-GCM's usual standards argument.

### D2 — Direct `getrandom = "0.4"` pin

Pinned once in `[workspace.dependencies]`, used via `getrandom::fill`
for both the 32-byte key and the 12-byte nonce. Reuses the
in-release-graph 0.4.3 — zero new lock entries, no new major
duplication.

### D3 — Envelope format: versioned raw-binary blob

```text
offset 0..4   magic   b"TBCE"                (TaskBoard Credential Envelope)
offset 4      version u8 = 1
offset 5..17  nonce   12 bytes (ChaCha20Poly1305 nonce, fresh per encryption)
offset 17..   ciphertext = plaintext || 16-byte Poly1305 tag
AAD           magic || version || credential_key(server, username) bytes
```

The AAD binds the header (magic/version cannot be stripped or
spliced without failing the tag) and the account (an envelope file
renamed onto another account's name fails closed as an auth error —
the confusion attack the filename identity would otherwise allow).
Decoding is length-checked first (short file → `EnvelopeCorrupt`),
then magic (`EnvelopeCorrupt`), then version (`EnvelopeVersion(u8)`),
then the tag (`EnvelopeAuth`). Raw binary, no base64/hex layer —
machine-owned files, no encoding dep.

### D4 — Key-file format

```text
b"TBCK" (4) || version u8 = 1 || 32 raw key bytes   (37 bytes total)
```

Wrong magic, wrong length, or wrong version → `KeyFileCorrupt`.
Format-discovered: a stray non-key file at the key path fails typed
instead of being used as 37 arbitrary bytes.

### D5 — Layout and path policy: derived from the db path, no new config keys

```text
<db-dir>/credential.key                     (0600, 37 bytes, machine-generated)
<db-dir>/credentials/<hex(credential_key)>.bin   (0600 envelopes)
```

`[nextcloud]` stays `deny_unknown_fields` with no new key; the
operator relocates the whole store by moving `[storage] db_path`
("key + ciphertext beside the db", per the ruling). `config.rs`
gains **one** definition-site accessor,
`AppConfig::credential_base_dir() -> &Path` (= `db_path().parent()`,
with the empty-parent edge mapped to `"."`); the store owns the two
file names from there. Filenames hex-encode the `credential_key`
output — lossless for arbitrary UTF-8 accounts (the garbage-input
guarantees of `credential_key` make the account an arbitrary string),
collision-free, filesystem-safe; a ~15-line hand-rolled encoder, no
new dep.

### D6 — Key-file ceremony

- **Generation on first use**: `store()` with the key file absent
  generates 32 bytes via `getrandom::fill`, writes with
  `OpenOptions::create_new(true)` + `.mode(0o600)` (unix
  `OpenOptionsExt` — safe API, `forbid(unsafe_code)` untouched; a
  create race surfaces as `AlreadyExists` → typed `StoreIo`),
  fsyncs, then proceeds. `load()` never generates.
- **0600 enforced on every use** (unix, `MetadataExt::mode()`):
  `load()` and `store()` both refuse a key file whose mode ≠ 0o600
  with `KeyFileInsecureMode` — **never an auto-chmod**, never a
  warning-and-continue. The operator fixes the mode; the error is the
  fence. On Windows there is no mode bit: enforcement is `cfg(unix)`,
  and the documented stance is the user-profile ACL inheritance
  (recorded in the ADR amendment and README).
- **Envelope writes** are 0600, and atomic: write to a temp sibling,
  fsync, `std::fs::rename` over the destination (rename-replaces-
  existing holds on both unix and Windows via
  `MOVEFILE_REPLACE_EXISTING`). A crash mid-`store` leaves either the
  old or no envelope, never a torn one; power-cut durability beyond
  the file-level fsync (no directory fsync) is accepted and noted —
  the failure mode is a typed re-login, not corruption.
- `store()` `create_dir_all`s the `credentials/` tree — `login` does
  not bootstrap the db directories, so the store cannot assume they
  exist.

### D7 — Read/write/delete semantics

- `load`: envelope absent → `NotFound` (regardless of key state —
  "nothing stored", identical to the keyring pre-login semantics);
  envelope present + key file absent → `KeyFileMissing` (loud: the
  remedy is re-login; silently regenerating would orphan the
  envelope behind an unverifiable key); key present + mode ok →
  decrypt; tag failure → `EnvelopeAuth` (wrong key and tampering are
  deliberately indistinguishable).
- `store`: key ceremony per D6, then encrypt with a **fresh 12-byte
  random nonce per write** (random-nonce usage per RFC 8439; this
  store writes on the order of once per login, unimaginably below
  the 2^32-message random-nonce discipline) and the atomic write per
  D6.
- `delete`: removes the envelope file; absent envelope →
  `NotFound` (mirrors the keyring `NoEntry` mapping). The key file is
  **never** deleted by `delete()` — it is account-neutral and may
  guard other accounts' envelopes.

### D8 — Failure-fence catalogue (additive `CredentialError` variants)

| Variant | Fence |
|---|---|
| `KeyFileMissing` | envelope exists, key file does not |
| `KeyFileInsecureMode` | key-file mode ≠ 0600 (unix) |
| `KeyFileCorrupt` | key file wrong magic/version/length |
| `EnvelopeCorrupt` | envelope short or wrong magic |
| `EnvelopeVersion(u8)` | envelope magic ok, version ≠ 1 (future formats fail loudly, never misdecode) |
| `EnvelopeAuth` | Poly1305 tag mismatch — wrong key or tamper |
| `StoreIo(#[source] std::io::Error)` | unexpected fs failure (perm denied, disk full, create race) |
| `Entropy(#[source] getrandom::Error)` | OS randomness failure during key/nonce generation |

`NotFound`/`Unsupported` are reused unchanged; `Backend` stays
keyring-specific (the file store never emits it — its Display
names the env alternative, which would be a lie here). Tests match
variant shape only; Display strings are for humans.

### D9 — Config surface

`CredentialStoreKind::File` (kebab `"file"`), doc: the
encrypted-file store for machines without a Secret Service (kiosks);
the default stays `Keyring`. Decode test mirrors `config.rs:1252`
(the env pattern); the `config` verb's display match
(`commands/config.rs:75`) gains the `"file"` arm. The
process-global semantics (the `config.rs:487` ruling comment) carry
over unchanged — one store selection per process, including the
connector-array world.

### D10 — Wiring (the port unmoved; the factories gain a parameter)

`resolve_credential_store(kind)` and `production_store(kind)` each
become `(kind, base_dir: &Path)`; all six call sites pass
`config.credential_base_dir()`. The trait, `AppPassword`, and every
handler seam are untouched (handlers already take
`Arc<dyn CredentialStore>`; `FakeCredentialStore` injections keep
compiling). The compiler enumerates every exhaustive match — no site
can be missed. The two-factory duplication is pre-existing debt;
both get the identical arm, no merge.

### D11 — Docs

- **ADR 0008 amendment section** (the house pattern): the third
  store, the ceremony, and the threat-model honesty paragraph from
  briefing §1.8 verbatim in substance — *protects against other
  local users, accidental world-readable copies, and env-var
  visibility; not against root or full-disk theft; strictly better
  than plaintext or env-in-unit-file; strictly available where the
  keyring is not; weaker than the keyring where it is.*
- **README.org**: the credential-store trio table (keyring default /
  env escape hatch / file kiosk store), the `credential_store =
  "file"` selection, the layout on disk, and the same honesty
  paragraph in user-facing wording.
- **CHANGELOG.md**: one Unreleased line.

### D12 — Test catalogue (TDD-first; lowest layer; one behaviour per test)

Layer 1 — pure codec fns (inline `#[cfg(test)]` where they live):

- envelope encode/decode round-trip under a fixed key;
- corrupt magic → `EnvelopeCorrupt`; truncated → `EnvelopeCorrupt`;
  version byte 2 → `EnvelopeVersion(2)`; flipped ciphertext bit →
  `EnvelopeAuth`; AAD account mismatch (decode under another
  account) → `EnvelopeAuth`;
- key-file parse: 37 good bytes parse; short/long/wrong-magic/
  wrong-version → `KeyFileCorrupt`;
- account→filename hex: lossless (arbitrary UTF-8 round-trips),
  distinct accounts → distinct names.

Layer 2 — the store over `tempdir` (extend `tests/secrets.rs`,
S-series numbering, `tests/common` fixtures):

- **S8 round-trip + per-account scoping**: store/load for two
  (server, username) pairs; cross-load invisible (mirrors S6);
  delete → `NotFound`; delete of absent → `NotFound`.
- **S9 key ceremony**: first `store` creates the key file (length
  37, mode 0600 — `cfg(unix)`); a second `store` does not regenerate
  it (key bytes unchanged); `load` with the key file removed (and
  envelope present) → `KeyFileMissing`; key file truncated →
  `KeyFileCorrupt`; envelope re-encrypted under a fresh key file →
  `EnvelopeAuth`.
- **S10 mode fence** (`cfg(unix)`): chmod 0644 → `load` errs
  `KeyFileInsecureMode`; `store` errs the same; the error persists
  until the operator restores 0600 (no auto-fix).
- **S11 no-silent-fallback** (the strongest form): with
  `TASKBOARD_APP_PASSWORD` set (under `env_lock`) and the **file**
  store selected with a corrupt envelope, `load` returns the typed
  error — never the env value; plus the wiring probe:
  `production_store(File, tempdir)` round-trips file-backed.
- **S12 leak floor**: the raw envelope bytes on disk contain neither
  the plaintext password nor the raw key bytes; `Debug` of
  `FileCredentialStore` and the `format!` of every new error variant
  carry no secret (S1-class floor).
- **S13 nonce freshness**: storing the same secret twice yields two
  envelope files with different nonce regions (structural read of
  the two files' nonce offsets) and different bytes overall.

Layer 3 — config decode (`config.rs` tests): `"file"` decodes to the
`File` kind (the `config.rs:1252` pattern); the display arm follows
the existing `config`-verb output conventions.

`cfg(unix)`-gated tests are platform-conditional by design, not
silent skips; the round-trip fences (S8/S9 minus mode, S11–S13) run
on Windows CI too.

### D13 — `cargo deny` expectation

**Zero `deny.toml` edits.** New licenses: `chacha20poly1305`,
`poly1305`, `universal-hash` = Apache-2.0 OR MIT; `aead` = MIT OR
Apache-2.0 — all inside the global allow. No new major-version
duplicates (`chacha20 0.10.2` and `getrandom 0.4.3` are shared).
Sources: crates.io only. No known RUSTSEC advisories against these
versions at plan time; stages 2/3 re-run `cargo deny check` as a
gate.

### D14 — Zeroization posture (honest scope)

The `zeroize` feature is on (the cipher's internal key schedule
zeroizes on drop). `AppPassword`'s heap `String` keeps its existing
posture (no zeroizing `String` in safe Rust — unchanged, out of
scope). The raw key bytes transiently held in our own buffers during
read/write are best-effort in `forbid(unsafe_code)` Rust; stated as
such in the code comment, not over-promised.

## 4. Stage-2 implementation order (TDD; commits standalone)

1. Pins: root `[workspace.dependencies]` +
   `taskboard-app/Cargo.toml`; `cargo deny check` + build green
   (D1/D2/D13).
2. Config: the `File` variant + decode test + display arm +
   `credential_base_dir()` + its test (D9/D5).
3. Pure layer: envelope/key-file codecs + their unit tests (D3/D4,
   D12 layer 1).
4. `FileCredentialStore` + S8/S9/S10 (D6/D7, D8).
5. Factory wiring + S11 (D10).
6. S12/S13 leak-floor + nonce tests.
7. Docs: ADR 0008 amendment, README, CHANGELOG (D11).
8. Full AGENTS §10 gate suite, including the CI-identical
   three-command llvm-cov sequence.

## 5. Risks

- **API drift**: `chacha20poly1305 0.11`/`aead 0.6` ride the 2025
  RustCrypto trait refresh (`hybrid-array` keys, reorganised
  traits); stage 2 verifies construction/encrypt/decrypt against the
  actual 0.11 docs. The operations used are exactly
  key-from-slice / encrypt-in-place / decrypt-in-place — no design
  exposure.
- **Windows**: rename-over-existing via `std::fs::rename` is
  documented to replace (MOVEFILE_REPLACE_EXISTING); mode
  enforcement is unix-only by design. Windows CI runs the
  mode-independent fences.
- **umask pathology**: `OpenOptions::mode(0o600)` applies `&~
  umask`; a hostile umask could weaken the created mode — the
  every-use enforcement fence (D6) turns that into a typed failure
  on the next operation, never a silent weaken.
- **Rebase adjacency**: `commands/mod.rs` and `config.rs` are the
  programme's contested files (the connector registry landed +571
  lines in `config.rs` since the round was framed). K12's diff there
  is small (one enum arm, one accessor, three match arms, six call
  sites) and stage 3 rebases onto `origin/main` as usual; verified
  the registry round did not touch store selection.
- **AAD account binding** makes hand-renamed envelope files fail
  closed (`EnvelopeAuth`) — intended, documented in D3.
- The branch already sits on `origin/main`'s tip (§ header); stage
  3's rebase is a no-op unless main moves again.

## 6. Diff estimate

| File | Delta |
|---|---|
| `crates/taskboard-app/src/secrets.rs` | +~260 (store + codecs + error variants) |
| `crates/taskboard-app/src/config.rs` | +~30 (variant, accessor, decode test) |
| `crates/taskboard-app/src/commands/mod.rs` + `commands/config.rs` | +~20 |
| `main.rs` / `ui.rs` / `repl.rs` (call-site args) | +~12 |
| `Cargo.toml` (root) / app `Cargo.toml` / `Cargo.lock` | +3 / +2 / +4 crates |
| `crates/taskboard-app/tests/secrets.rs` | +~220 (S8–S13) |
| `docs/adr/0008-…org`, `README.org`, `CHANGELOG.md` | +~60 combined |

Total ≈ **600 lines across ~11 files** (size **M** on the house
scale, as dispatched).

## 7. Bookkeeping

Stage 3 ships the PR with `Closes #159` + `Closes #126` (the
implementation plan §6 map — #126's open remainder is exactly this
store). Stages 1–2 make no tracker edits (MD-4). The plan commit is
`docs(plan): encrypted-file credential store`; no push at this stage.
