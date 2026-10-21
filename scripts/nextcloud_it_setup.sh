#!/usr/bin/env bash
# Tier 2 integration environment manager (docs/testing_strategy.org §8).
#
# Starts a throwaway, loopback-only Nextcloud (docker/nextcloud-test),
# installs the Deck app, creates the `taskboard-it` test user, mints an app
# password, and prints an `export` block for the Tier 2 tests.
#
# Usage:
#   scripts/nextcloud_it_setup.sh up [--ci]     # default subcommand
#   eval "$(scripts/nextcloud_it_setup.sh up)"
#   scripts/nextcloud_it_setup.sh down
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="$ROOT/docker/nextcloud-test/compose.yaml"
# Instance suffix: an empty default (the original single instance) or
# e.g. "-c5a" — scopes the compose project and the container name so
# several throwaway instances coexist (the two-connector tier test
# owns two); the exported variable names follow the export prefix.
SUFFIX="${TASKBOARD_IT_SUFFIX:-}"
PROJECT="taskboard-it${SUFFIX}"
CONTAINER="taskboard-it-nextcloud${SUFFIX}"
PORT="${TASKBOARD_IT_PORT:-8370}"
# Pinned Deck install (the 1.17.5 contract-fixture gate): empty default =
# today's `occ app:install deck` (latest app-store version); a tag like
# `v1.17.5` installs that GitHub release tarball instead.
DECK_VERSION="${DECK_VERSION:-}"
# The compose file interpolates the bare names; re-export the
# taskboard-prefixed overrides under them (empty SUFFIX = the
# singleton's original names, byte-for-byte).
export SUFFIX
export PORT
BASE_URL="http://127.0.0.1:$PORT"
IT_USER="taskboard-it"
# Throwaway by design; only used inside the ephemeral container.
export OC_PASS="${TASKBOARD_IT_USER_PASSWORD:-taskboard-it-user-pw}"
READY_DEADLINE=180

die() { echo "error: $*" >&2; exit 1; }

occ() {
    # docker compose exec does not inherit the host environment, so the
    # user-creation password must be forwarded explicitly (NC_PASS/OC_PASS
    # is what `occ user:add --password-from-env` reads inside the container).
    docker compose -p "$PROJECT" -f "$COMPOSE_FILE" exec -T -u www-data \
        -e NC_PASS="$OC_PASS" -e OC_PASS="$OC_PASS" \
        nextcloud php occ "$@"
}

poll_ready() {
    python3 - "$BASE_URL" "$READY_DEADLINE" <<'PY'
import json, sys, time, urllib.request

base, deadline = sys.argv[1], float(sys.argv[2])
end = time.monotonic() + deadline
while time.monotonic() < end:
    try:
        with urllib.request.urlopen(base + "/status.php", timeout=5) as r:
            if json.load(r).get("installed") is True:
                sys.exit(0)
    except Exception:
        pass
    time.sleep(2)
sys.exit(f"nextcloud not ready within {deadline:.0f}s")
PY
}

app_password_command() {
    # Flag names drift across Nextcloud releases; ask the image, never guess.
    if occ list 2>/dev/null | grep -w 'user:add-app-password' >/dev/null; then
        echo "user:add-app-password"
    elif occ list 2>/dev/null | grep -w 'app-password:assign' >/dev/null; then
        echo "app-password:assign"
    else
        die "image supports neither 'user:add-app-password' nor 'app-password:assign'; cannot mint an app password"
    fi
}

install_deck() {
    if [ -n "$DECK_VERSION" ]; then
        install_deck_pinned
        return
    fi
    if occ app:list 2>/dev/null | grep -E '^\s*- deck:' >/dev/null; then
        occ app:enable deck || true
        return
    fi
    # App-store downloads are occasionally flaky; retry once, then fail loud.
    occ app:install deck || occ app:install deck
}

install_deck_pinned() {
    # Pinned-release install: download the GitHub release tarball inside
    # the container (curl is present — the healthcheck uses it), extract
    # to custom_apps, enable. Fails loud on a failed download or a
    # version mismatch — never a silently wrong-tier instance.
    #
    # The artifact lives in the `nextcloud-releases` org with a
    # versioned asset name (deck-vX.Y.Z.tar.gz); the app store's
    # download links point there for all current Deck releases.
    local expected="${DECK_VERSION#v}"
    if ! docker compose -p "$PROJECT" -f "$COMPOSE_FILE" exec -T nextcloud bash -c '
        set -euo pipefail
        curl -fsSL "$1" -o /tmp/deck.tar.gz
        rm -rf /var/www/html/custom_apps/deck
        mkdir -p /var/www/html/custom_apps
        tar -xzf /tmp/deck.tar.gz -C /var/www/html/custom_apps
        test -d /var/www/html/custom_apps/deck
        chown -R www-data:root /var/www/html/custom_apps/deck
    ' _ "https://github.com/nextcloud-releases/deck/releases/download/${DECK_VERSION}/deck-${DECK_VERSION}.tar.gz"; then
        die "could not fetch/extract the Deck ${DECK_VERSION} release tarball"
    fi
    occ app:enable deck
    local got
    got="$(occ app:list | awk '/ - deck:/{print $3; exit}')"
    [[ "$got" = "$expected" ]] \
        || die "Deck version mismatch: wanted ${expected}, instance reports '${got}'"
}

create_it_user() {
    # NB: `occ user:info` exits 0 even for a missing account; match the
    # uid in its output instead.
    if occ user:info "$IT_USER" 2>/dev/null | grep -w "$IT_USER" >/dev/null; then
        return
    fi
    occ user:add --password-from-env "$IT_USER"
}

mint_app_password() {
    local cmd output token
    cmd="$(app_password_command)"
    # Feed the account password on stdin so the minted token gets full API
    # capabilities (skipping the prompt yields a limited token).
    output="$(printf '%s\n' "$OC_PASS" | occ "$cmd" "$IT_USER")"
    # The token is printed on the line after "app password:". Validate the
    # shape and fail loudly rather than emitting a guess.
    token="$(printf '%s\n' "$output" \
        | awk '/app password:/{getline; gsub(/^[ \t]+|[ \t]+$/, ""); print; exit}')"
    [[ "$token" =~ ^[A-Za-z0-9]{20,}$ ]] \
        || die "could not parse app password from output: $output"
    printf '%s' "$token"
}

emit_exports() {
    local token="$1" ci="$2" prefix="${EXPORT_PREFIX:-TASKBOARD_IT_DOCKER}"
    cat <<EOF
export ${prefix}_URL=$BASE_URL
export ${prefix}_USER=$IT_USER
export ${prefix}_TOKEN=$token
EOF
    if [ "$ci" = "--ci" ]; then
        {
            echo "${prefix}_URL=$BASE_URL"
            echo "${prefix}_USER=$IT_USER"
            echo "${prefix}_TOKEN=$token"
        } >>"${GITHUB_ENV:?GITHUB_ENV must be set in CI}"
    fi
}

cmd_up() {
    local ci="${1:-}"
    command -v docker >/dev/null || die "docker is required"
    # Everything noisy goes to stderr: stdout is eval'd as an export block.
    # --wait needs a compose version with healthcheck support; fall back to
    # plain up + host-side poll otherwise.
    docker compose -p "$PROJECT" -f "$COMPOSE_FILE" up -d --wait >&2 \
        || { docker compose -p "$PROJECT" -f "$COMPOSE_FILE" up -d >&2; poll_ready >&2; }
    poll_ready >&2
    install_deck >&2
    create_it_user >&2
    token="$(mint_app_password)"
    emit_exports "$token" "$ci"
}

cmd_down() {
    docker compose -p "$PROJECT" -f "$COMPOSE_FILE" down -v --remove-orphans
}

case "${1:-up}" in
    up)
        shift
        cmd_up "${1:-}"
        ;;
    down) cmd_down ;;
    *)
        echo "usage: $0 [up [--ci] | down]" >&2
        exit 2
        ;;
esac
