# Netbook (D250) Kiosk Test-Environment Setup Report — 2026-10-21

Prepared the `aspire-net-srv` test box (the D250-0Dk wall-display unit,
issue #233) for the on-device kiosk performance spike. Everything was
installed by executing the merged K9 bring-up runbook
(`docs/kiosk_bringup_runbook.org` §2/§6) plus the seat-session
mechanism the runbook references but does not specify.

## System facts (surveyed 2026-10-21)

| Fact | Value |
|---|---|
| Host | `aspire-net-srv` (192.168.1.109), user `agere` (uid 1000) |
| OS | Debian 12.15, i386 userland, kernel 6.1.0-54-686-pae |
| CPU | Intel Atom N270 @ 1.60 GHz (32-bit only, no x86_64) |
| GPU | Intel 945GME (gen3), LVDS panel 1024×600 |
| RAM / disk | 987 MB / 23 GB (21 GB free) |
| Root access | `sudo` installed this session; NOPASSWD drop-in `/etc/sudoers.d/agere-nopasswd` (user-bootstrapped via `su -c`) |

## What was installed

1. **Packages** — runbook §2 list (`--no-install-recommends xorg xinit
   libinput-tools libgl1 mesa-utils libegl1 fonts-dejavu-core python3
   python3-minimal`) plus `openbox` (seat WM) and `x11vnc`
   (observation). **Plus `xserver-xorg-video-intel` — required, see
   finding F1.**
2. **Appliance user + dirs** (runbook §6):
   `taskboard` system user (uid 999, `/var/lib/taskboard`,
   nologin); `/var/lib/taskboard` owned by it; `/etc/taskboard/`.
3. **Config** `/etc/taskboard/taskboard.toml` — `[app] mode = "kiosk"`,
   `[sync] backend = "standalone"`, `[storage] db_path =
   "/var/lib/taskboard/taskboard.db"`. No `[kiosk] fixed_board` pin yet.
4. **Unit** `/etc/systemd/system/taskboard-kiosk.service` — verbatim
   from `contrib/taskboard-kiosk.service` (PR #380), mode 0644, plus
   `/etc/default/taskboard-kiosk` containing `DISPLAY=:0`.
   **Deliberately left `disabled`** until `/usr/local/bin/taskboard`
   exists — enabling now would 203/EXEC-restart-loop every 5 s.
5. **Seat session** (the runbook's unspecified xinit line, F2):
   - `/etc/systemd/system/getty@tty1.service.d/autologin.conf`:
     `agetty -a agere --noclear %I $TERM` (autologin, no password).
   - `~agere/.bash_profile`: if `$SSH_CONNECTION`/`$DISPLAY` set →
     source `.profile` (SSH untouched); else `exec startx -- -nolisten
     tcp`. X crash ⇒ login session ends ⇒ getty respawns ⇒ self-healing
     seat.
   - `~agere/.xinitrc`: `xset s off -dpms`; `xsetroot -solid black`;
     `xhost +SI:localuser:taskboard` (appliance user display access
     without sharing the xauth cookie); `x11vnc -localhost -forever
     -shared -rfbport 5900 -display "$DISPLAY"`; `exec
     openbox-session`.
   - `graphical.target` was already the default target.

## Verification results (post-reboot, all green)

- Xorg `:0` via the **intel DDX** on vt1; openbox + x11vnc stable;
  `getty@tty1` `NRestarts=0`.
- `glxinfo`: OpenGL 2.1 Mesa 22.3.6, renderer **i915 (945GME)** — see
  finding F3.
- `taskboard` user reaches `:0` (`xdpyinfo` under `sudo -u taskboard`).
- DPMS disabled, screensaver off; VNC listener bound to
  127.0.0.1:5900 only.
- Memory baseline with the whole seat up: **241 MB used / 746 MB
  available** — comfortable headroom against the 200 MB app-RSS budget.

## Findings (runbook/ADR feedback)

- **F1 — runbook §2 package list is insufficient on 945GME.** With
  `--no-install-recommends`, the `xorg` metapackage brings only the
  modesetting driver, which hard-requires glamor; glamor refuses the
  GMA 950 ("glamor requires at least 128 instructions (64 reported)")
  and Xorg dies at ScreenInit. `xserver-xorg-video-intel` (intel DDX,
  no glamor requirement) is mandatory on this unit. Proposed runbook
  edit: add it to the §2 install line.
- **F2 — runbook §6 seat-session gap.** §2 promises "the unit
  supervises its own seat session (§6)" and a wayland note references
  a "seat session line in §6", but §6 never spells the X11 xinit
  mechanism out. This box's working answer (autologin getty →
  `.bash_profile` guard → `startx` → `.xinitrc`) is documented above
  and is a candidate to become the runbook's X11 seat line. Note the
  kiosk unit itself stays display-agnostic (env file) as designed.
- **F3 — "no gen3 hardware GL" premise is wrong on this unit.**
  Debian 12 Mesa 22.3.6 still ships the classic i915 (DRI1) driver:
  the 945GME exposes **hardware OpenGL 2.1**. ADR 0015's corollary and
  runbook §1 say modern Mesa ships no gen3 hardware GL. femtovg needs
  GL ≥ 3.0, so the software-renderer fallback likely still decides the
  spike, but the on-device measurement should record both paths
  (default `SLINT_BACKEND` unset vs explicitly forced software) since
  the premise the fallback rests on no longer holds as stated.
- **F4 — stale X-lock pitfall.** The crash loop left `/tmp/.X0-lock`;
  `startx` then silently migrated to `:1`, breaking the env file's
  `DISPLAY=:0`. Fixed by lock cleanup; `.xinitrc` is now
  display-agnostic (`"$DISPLAY"`).

## Operations cheat-sheet (this box)

```sh
ssh aspire-net-srv                                  # agere (passwordless sudo)
ssh -L 5900:localhost:5900 aspire-net-srv           # then vnc://localhost:5900 to watch the wall
sudo systemctl restart getty@tty1.service           # bounce the seat (X+openbox+x11vnc)
XAUTH=$(pgrep -a -x Xorg | grep -o '\-auth [^ ]*' | cut -d' ' -f2)
DISPLAY=:0 XAUTHORITY="$XAUTH" glxinfo | grep renderer
journalctl -u taskboard-kiosk -f                    # once the unit is enabled
```

## i686 cross-build lane (§5) — executed 2026-10-21, binary deployed

The lane runs on the dev box in a **Debian 12 container** (host glibc
2.43 > target 2.36, §5.4). The image recipe is committed at
`docker/i686-cross-build/Dockerfile` (image
`taskboard-build:debian12-i686`) and the invocation is wrapped by
`scripts/i686-cross-build.sh` (output cache:
`<repo-sibling>/target-i686/`):

```sh
docker build -t taskboard-build:debian12-i686 docker/i686-cross-build/
scripts/i686-cross-build.sh          # → target-i686/i686-…/release/taskboard
```

- Artifact: `target/i686-…/release/taskboard`, ELF 32-bit i386,
  stripped, 35.8 MB; highest glibc symbol requirement **2.35** ≤ 2.36
  (verified via `objdump -T`).
- Installed on the D250 as `/usr/local/bin/taskboard`; unit **enabled
  and active**; window manager shows the 1024×600 fullscreen
  "taskboard" window; the structured `first-frame` moment fired
  (§7 slot 3's readiness predicate works on-device).
- **First RSS smoke**: 135 MB with an empty DB at ~1 min uptime —
  inside the ≤ 200 MB budget before the fixture (slot 1 needs the
  N=10 000 DB for the real verdict).
- **F5 — runtime gap in runbook §2**: winit's X11 backend panics
  without `libxkbcommon-x11-0` (runtime dlopen, no package dependency)
  — added to the netbook; runbook §2 should list it.
- **F6 — GL path observed**: as the `taskboard` system user the app
  cannot open `/dev/dri/card0`, so libGL logs `i915 … Permission
  denied` and falls back to software GL. That *is* the runbook's
  planned lane, but it is reached by permission, not by selection —
  the spike should decide whether that fallback is the sanctioned
  posture (e.g. keep the user out of `video`/`render`) or whether the
  F3 hardware-GL 2.1 path should be measured too.
- Cosmetic: no user session bus for uid 999 (xdg-settings watch warns,
  harmless); winit guessed scale factor 1.25 on the 1024×600 panel —
  UI scale may need the `[ui] scale` override for legibility.

## Measurement results (2026-10-21) — the four §7 slots

| # | Verdict | Budget | Measured | Result |
|---+---------+--------+----------+--------|
| 1 | RSS steady/peak | ≤ 200 MB | 49 712 kB steady / 50 160 kB peak (headless daemon, N=10 000 fixture, 13 samples `"steady": true`) | **within (~25 %)** |
| 2 | Full/partial repaint | ≤ 50 / ≤ 20 ms | K6 bench run **on-device** (i686 cross-built criterion binary, software renderer, 1366×768 frame): full 31.1 ms @ N=100 / 46.4 ms @ N=1 000 / 202.8 ms @ N=10 000; partial 110.8 / 659.0 / 6 141 ms | **full within at board-scale N ≤ 1 000 (marginal at 1 000); partial fails as measured — dominated by the D6 dirt (see below)** |
| 3 | Cold start | ≤ 5 s | 2 106.7 ms spawn → first-frame, X11, N=10 000 fixture | **within (~42 %)** |
| 4 | Sync-cycle CPU share | recorded | on-device `last_cycle_ms` = **685 ms**/cycle (Wi-Fi-LAN http to the dev-side Nextcloud; 30 s poll; cycles clean: pulls 5/cycle, failures 0) | recorded |

### Dev-machine criterion references (run 2026-10-21, release profile)

K6 refresh+draw (`cargo bench -p taskboard-ui-slint`; software
renderer, headless seam, frame 1366×768):

| bench | 100 | 1 000 | 10 000 |
|-------+-----+-------+--------|
| refresh (load→reconcile→push) | 0.030 ms | 0.247 ms | 5.15 ms |
| draw-full | 3.34 ms | 8.61 ms | **60.5 ms** |
| draw-partial | 25.3 ms | 197.9 ms | 1 851 ms |

- `draw-full` at N=10 000 is **over the 50 ms budget line on the dev
  machine already** — the reference to weigh at spike exit (the wall
  shows one board; 10 k tasks/board is the extreme fixture).
- The partial legs confirm the **D6 recorded finding**: the partial
  dirt computes ≈97 % of the full frame (whole-`VecModel`-replacement
  dirt), so "partial" converges to a full redraw and worse.

On-device draw (the slot-2 spike leg — same benches, i686
cross-built criterion binary run on the D250; the dev-vs-device ratio
narrows from ~9× at N=100 to ~3.4× at N=10 000):

| bench | dev | D250 |
|-------+-----+------|
| draw-full/100 | 3.34 ms | 31.1 ms |
| draw-full/1 000 | 8.61 ms | 46.4 ms |
| draw-full/10 000 | 60.5 ms | 202.8 ms |
| draw-partial/100 | 25.3 ms | 110.8 ms |
| draw-partial/1 000 | 197.9 ms | 659.0 ms |
| draw-partial/10 000 | 1 851 ms | 6 141 ms |

Reading for the spike report: the **full-frame budget (≤ 50 ms) holds
on the N270 up to ~1 000 tasks/board** (31 ms at 100, 46 ms at 1 000,
203 ms at the 10 k extreme). The **partial budget (≤ 20 ms) fails at
every scale as measured — but the measurement is dominated by the D6
dirt**: with the partial region at 97 % of the frame, the partial legs
are full redraws plus dirty-region overhead, not a true partial. The
honest spike verdict is therefore: partial is *unmeasured as a true
partial* until the D6 dirt-tracking changes land (K13's chunk or a
dedicated follow-up); the numbers above bound it from above.

K7 mapping (S4 CPU legs, `cargo bench -p taskboard-sync-nextcloud`):
`map_card` 10 000 = 1.25 ms; `plan_pushes` steady 10 000 = 0.40 ms,
burst 10 000 = 5.83 ms; stacks/labels/boards sub-microsecond.
Both bench groups are Bencher-tracked; numbers above are the medians
from `target/criterion/*/new/estimates.json`.

### Slot-4 on-device setup (live sync path)

- Dev-side throwaway Nextcloud: `scripts/nextcloud_it_setup.sh up`
  (Deck installed, user `taskboard-it`, script-minted app password —
  **no user credentials involved**). The instance publishes to
  loopback only; a detached user-space forward exposes it to the LAN:
  `setsid nohup socat TCP4-LISTEN:8370,bind=192.168.1.107,fork,reuseaddr
  TCP4:127.0.0.1:8370 &` (stop with `pkill -f "socat TCP4-LISTEN:8370"`;
  teardown of the instance: `scripts/nextcloud_it_setup.sh down`).
- Nextcloud `trusted_domains` gained `192.168.1.107`
  (`occ config:system:set trusted_domains 2 --value=192.168.1.107`) —
  loopback-only trust answers 400 to the LAN vhost.
- Netbook `/etc/taskboard/taskboard.toml`: `[sync] backend =
  "nextcloud"` (via `connect --switch-backend`), `[nextcloud]`
  server_url/username + `credential_store = "file"`; app password in
  the K12 file store (`credential.key` 0600 + `credentials/` AEAD
  envelopes under `/var/lib/taskboard/`). Board `Kiosk Wall` created
  and **published** (`boards create` + `boards publish`) — publishing
  mints the remote Deck board and binds; until a board is bound the
  sync actor logs `cycle skipped: no board bound for this connector`
  and `sync` times out with no counters.
- `ufw` inactive on both ends — no firewall change was needed; the
  app-password transport is cleartext http inside the LAN (the app
  warns; acceptable for the throwaway test instance).

### F7 — the `connect` ceremony rewrites the config 0600 root:root
(filed as [issue #386](https://github.com/D4id4los/taskboard/issues/386))

`taskboard connect` (run as root, writing `/etc/taskboard/
taskboard.toml`) tightened the file to `0600 root:root` after the
runbook's `install -m 0644`. The kiosk appliance runs as the
`taskboard` user and can no longer read its own config → decode
failure → `Restart=always` loop. Symptom surfaces as `kind="decode"`
("configuration rejected"), which reads as a config-content bug but is
a permission denial. Workaround on the box: `chmod 0644
/etc/taskboard/taskboard.toml` after any root-run `connect`. Proposed
follow-up: `connect` should preserve (or not tighten) an existing
config's mode — candidate GitHub issue.

### F8 — a mid-commit container build produced a stale-schema binary

The first deployed i686 binary rejected `credential_store = "file"`
while accepting `env`/`keyring`: it was compiled in the container
*while the working tree was mid-commit* (config.rs changed at 16:48:37,
the container compile of `taskboard-app` ran ≈16:44–16:49 — the tree
moved under the build). Lesson for the lane: **do not cross-build from
a live checkout while other rounds are committing**; verify a clean
tree (`git status`) and, when in doubt, `touch` the app crate's
sources to defeat any mtime-coincidence fingerprint. The rebuild from
the settled tree behaves correctly.

## Appendix — full command sequence (for future agents)

Everything below was executed 2026-10-21 and reproduced the state this
report describes. The D250 is `aspire-net-srv` (192.168.1.109, user
`agere`, passwordless sudo); the dev box is on the same LAN at
192.168.1.107.

### A. Netbook seat + appliance (one-time; root via sudo)

```sh
# packages (§2 + the fixes: intel DDX per F1, xkbcommon per F5)
ssh aspire-net-srv
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
    xorg xinit libinput-tools libgl1 mesa-utils libegl1 \
    fonts-dejavu-core python3 python3-minimal openbox x11vnc \
    xserver-xorg-video-intel libxkbcommon-x11-0

# appliance user + dirs + config + unit (§6)
sudo useradd --system --home-dir /var/lib/taskboard --shell /usr/sbin/nologin taskboard
sudo install -d -o taskboard -g taskboard /var/lib/taskboard
sudo install -d -m 0755 /etc/taskboard
sudo install -m 0644 contrib/taskboard-kiosk.service /etc/systemd/system/
printf 'DISPLAY=:0\n' | sudo tee /etc/default/taskboard-kiosk
sudo systemctl daemon-reload
# config: [app] mode="kiosk"; [storage] db_path=/var/lib/taskboard/taskboard.db
# → /etc/taskboard/taskboard.toml, mode 0644 (the taskboard user must read it — F7)

# seat session: autologin getty on tty1 → startx → openbox
sudo systemctl enable getty@tty1   # + drop-in /etc/systemd/system/getty@tty1.service.d/autologin.conf:
#   [Service]
#   ExecStart=
#   ExecStart=-/sbin/agetty -a agere --noclear %I $TERM
# ~agere/.bash_profile: SSH guard → .profile; else: exec startx -- -nolisten tcp
# ~agere/.xinitrc: xset s off -dpms; xhost +SI:localuser:taskboard;
#   x11vnc -localhost -forever -shared -rfbport 5900 -display "$DISPLAY" &;
#   exec openbox-session
# full file texts: §"What was installed" above

# verify (reboot), then deploy the binary and start the appliance
sudo install -m 0755 taskboard /usr/local/bin/taskboard
sudo systemctl enable --now taskboard-kiosk.service
```

### B. The i686 cross-build (per binary)

```sh
docker build -t taskboard-build:debian12-i686 docker/i686-cross-build/
scripts/i686-cross-build.sh
# → target-i686/i686-unknown-linux-gnu/release/taskboard
# verify: file (ELF 32-bit i386) + objdump -T | grep GLIBC_ (max ≤ 2.36)
scp target-i686/i686-unknown-linux-gnu/release/taskboard \
    aspire-net-srv:/tmp/taskboard
ssh aspire-net-srv 'sudo install -m 0755 /tmp/taskboard /usr/local/bin/taskboard'
# NOT while other rounds are committing — finding F8.
```

### C. The N=10 000 fixture (once; dev machine → netbook)

```sh
cargo build --release -p taskboard-storage-sqlite --example build-bench-fixture
./target/release/examples/build-bench-fixture \
    --out ~/taskboard-kiosk-fixture/taskboard.db --tasks 10000
ssh aspire-net-srv 'mkdir -p ~/kiosk-fixture'
scp ~/taskboard-kiosk-fixture/taskboard.db aspire-net-srv:kiosk-fixture/
scp scripts/process_smoke.py aspire-net-srv:kiosk-fixture/
```

### D. The four §7 measurement slots

```sh
# slots 1+3 on the netbook (slot 3 needs the seat's display env):
ssh aspire-net-srv
python3 ~/kiosk-fixture/process_smoke.py rss \
    --taskboard /usr/local/bin/taskboard --no-build \
    --fixture ~/kiosk-fixture/taskboard.db
export XAUTHORITY=$(pgrep -a -x Xorg | grep -o '\-auth [^ ]*' | cut -d' ' -f2)
export DISPLAY=:0
python3 ~/kiosk-fixture/process_smoke.py cold-start \
    --taskboard /usr/local/bin/taskboard --no-build \
    --fixture ~/kiosk-fixture/taskboard.db
# tip: stop taskboard-kiosk for the cold-start run so the appliance
# does not compete for the CPU; start it again afterwards.

# slot 2 on-device: cross-build the K6 bench and run it there
scripts/i686-cross-build.sh --bench refresh_draw   # extra args pass through
scp target-i686/i686-unknown-linux-gnu/release/deps/refresh_draw-* \
    aspire-net-srv:/tmp/refresh_draw
ssh aspire-net-srv 'chmod +x /tmp/refresh_draw && cd /tmp && ./refresh_draw --bench draw'

# dev-machine reference legs (slot 2 + slot 4 CPU):
cargo bench -p taskboard-ui-slint          # K6 refresh/draw (medians in target/criterion)
cargo bench -p taskboard-sync-nextcloud    # K7 mapping

# slot 4 on-device: live sync against a throwaway Nextcloud (dev side)
scripts/nextcloud_it_setup.sh up           # prints the app-password export block
setsid nohup socat TCP4-LISTEN:8370,bind=192.168.1.107,fork,reuseaddr \
    TCP4:127.0.0.1:8370 </dev/null >/dev/null 2>&1 &
docker exec -u www-data taskboard-it-nextcloud php occ config:system:set \
    trusted_domains 2 --value=192.168.1.107
# netbook: [nextcloud] credential_store="file", then (password via stdin):
printf '%s' "$TASKBOARD_IT_DOCKER_TOKEN" | sudo taskboard --config \
    /etc/taskboard/taskboard.toml connect \
    --server http://192.168.1.107:8370 --user taskboard-it --switch-backend
sudo chmod 0644 /etc/taskboard/taskboard.toml   # F7 workaround
sudo systemctl restart taskboard-kiosk
sudo -u taskboard taskboard --config /etc/taskboard/taskboard.toml \
    boards create "Kiosk Wall"
sudo -u taskboard taskboard --config /etc/taskboard/taskboard.toml boards publish
sudo -u taskboard taskboard --config /etc/taskboard/taskboard.toml status --json
# → connectors[0].health.last_cycle_ms is the slot-4 reading
```

## Remaining before the four §7 measurement slots can be filled

1. **Slot 2's true-partial measurement** — the on-device partial legs
   are bounded above by the D6 dirt (≈97 % of frame); a true partial
   verdict needs the dirt-tracking change. Cross-built bench binary
   method note: `cargo build --release --target i686-unknown-linux-gnu
   -p taskboard-ui-slint --benches` in the container lane, deploy
   `target/i686-…/release/deps/refresh_draw-*`, run on-device
   (`./refresh_draw --bench draw`; criterion writes to
   `./target/criterion` under its CWD).
