// SPDX-License-Identifier: AGPL-3.0-only
//! The dashboard route table (K11 design §7 rule 1): a pure,
//! `(method, path)`-keyed resolution over the capability-token URL
//! space. v1 registers GET arms only; every miss — including a wrong
//! token — collapses into the same [`Resolution::NotFound`], because a
//! wrong capability must be indistinguishable from a nonexistent path.

/// The stylesheet's path segment; a redesign ships `style-v2.css` (the
/// version in the URL is the cache-buster — Q8's one-year `max-age`).
pub const STYLE_SEGMENT: &str = "style-v1.css";

/// The route arms a resolved path can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `/d/<token>/` — the sync-health + counters overview.
    Index,
    /// `/d/<token>/board/<board-uuid>` — one board's stacks.
    Board(uuid::Uuid),
    /// `/d/<token>/<STYLE_SEGMENT>` — the versioned stylesheet.
    Stylesheet,
}

/// The outcome of one `(method, path)` lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// A route arm to serve.
    Serve(Route),
    /// No such resource — uniformly, for an unknown path, a wrong
    /// token, and a malformed board id alike.
    NotFound,
    /// A known path hit with a non-GET method: the future forms lane's
    /// honest `405`, without serving anything today.
    MethodNotAllowed,
    /// The bare `/d/<token>` index (no trailing slash) — never served
    /// as HTML, because the pages' links are relative: the client is
    /// sent to the canonical `/d/<token>/` first.
    Redirect,
}

/// Resolves one request's `(method, path)` against the token. Pure —
/// string-and-uuid work only, so the table's behaviour is unit-testable
/// without HTTP types.
#[must_use]
pub fn resolve(method: &str, path: &str, token: &str) -> Resolution {
    // Empty segments (`//`) are misses, not flexible separators.
    let Some(rest) = path.strip_prefix("/d/").filter(|rest| !rest.contains("//")) else {
        return Resolution::NotFound;
    };
    let (token_segment, rest) = match rest.split_once('/') {
        Some((token, rest)) => (token, Some(rest)),
        None => (rest, None),
    };
    // The token gates the path space before the method does: a wrong
    // capability is a 404 in every method, never a 405 leak.
    if !constant_time_eq(token_segment, token) {
        return Resolution::NotFound;
    }
    let route = match rest.map_or("", |segment| segment.trim_end_matches('/')) {
        "" => Route::Index,
        "board" => return Resolution::NotFound, // no id, no board
        segment => match segment.strip_prefix("board/") {
            Some(id) => match uuid::Uuid::parse_str(id) {
                Ok(id) => Route::Board(id),
                Err(_) => return Resolution::NotFound,
            },
            None if segment == STYLE_SEGMENT => Route::Stylesheet,
            None => return Resolution::NotFound,
        },
    };
    match method {
        // HEAD rides the GET arm at the HTTP layer (hyper answers it
        // from the GET handler); every other method is the typed 405.
        "GET" | "HEAD" if rest.is_none() => {
            // The bare `/d/<token>` redirects to the canonical
            // trailing-slash index instead of serving it in place —
            // the pages' links (`board/…`, `style-v1.css`) are
            // relative, so a slash-less HTML response would carry
            // links the browser resolves against `/d/` (broken 404s).
            Resolution::Redirect
        }
        "GET" | "HEAD" => Resolution::Serve(route),
        _ => Resolution::MethodNotAllowed,
    }
}

/// Length-constant equality for the capability comparison. A plain
/// `==` short-circuits on the first differing byte, leaking the token
/// one byte per probe to a timing attacker on the LAN; this fold never
/// does. (The threat is modest — a loopback-only default and a
/// high-entropy token — but the comparison is free.)
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |diff, (x, y)| diff | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::{Resolution, Route, STYLE_SEGMENT, resolve};

    const TOKEN: &str = "abcdef0123456789";

    #[test]
    fn get_arms_resolve() {
        assert_eq!(
            resolve("GET", "/d/abcdef0123456789/", TOKEN),
            Resolution::Serve(Route::Index)
        );
        // The bare (trailing-slash-less) index redirects to the
        // canonical form instead of serving HTML in place.
        assert_eq!(
            resolve("GET", "/d/abcdef0123456789", TOKEN),
            Resolution::Redirect
        );
        let id = uuid::Uuid::nil();
        assert_eq!(
            resolve("GET", &format!("/d/{TOKEN}/board/{id}"), TOKEN),
            Resolution::Serve(Route::Board(id))
        );
        assert_eq!(
            resolve("GET", &format!("/d/{TOKEN}/{STYLE_SEGMENT}"), TOKEN),
            Resolution::Serve(Route::Stylesheet)
        );
    }

    /// The capability discipline: a wrong token is indistinguishable
    /// from a nonexistent path — uniformly 404, in every method and on
    /// every path shape.
    #[test]
    fn a_wrong_token_is_404_like_any_other_miss() {
        for path in [
            "/d/wrongtoken/",
            "/d/wrongtoken",
            &format!("/d/wrongtoken/board/{}", uuid::Uuid::nil()),
            &format!("/d/wrongtoken/{STYLE_SEGMENT}"),
        ] {
            for method in ["GET", "POST", "PUT", "DELETE", "HEAD"] {
                assert_eq!(
                    resolve(method, path, TOKEN),
                    Resolution::NotFound,
                    "{method} {path} must be a uniform miss"
                );
            }
        }
    }

    #[test]
    fn unknown_paths_are_404_even_with_the_right_token() {
        for path in [
            "/",
            "/d",
            &format!("/d/{TOKEN}/nope"),
            &format!("/d/{TOKEN}/board"),
            &format!("/d/{TOKEN}/board/not-a-uuid"),
            &format!("/d/{TOKEN}/style-v2.css"),
            &format!("/d/{TOKEN}/../etc"),
            &format!("/d/{TOKEN}//"),
        ] {
            assert_eq!(
                resolve("GET", path, TOKEN),
                Resolution::NotFound,
                "{path} must be a miss"
            );
        }
    }

    /// Non-GET on a *valid* path is the honest 405 (the future forms
    /// lane's key stays honest); HEAD rides the GET arm.
    #[test]
    fn non_get_on_a_valid_path_is_405_but_head_rides_get() {
        let index = format!("/d/{TOKEN}/");
        for method in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS"] {
            assert_eq!(
                resolve(method, &index, TOKEN),
                Resolution::MethodNotAllowed,
                "{method} on the index must be 405"
            );
        }
        assert_eq!(
            resolve("HEAD", &index, TOKEN),
            Resolution::Serve(Route::Index)
        );
        // A valid path behind a wrong token stays 404 even for these —
        // checked in `a_wrong_token_is_404_like_any_other_miss`.
    }

    /// The slash-less index is a valid path: non-GET is still the 405,
    /// HEAD still rides the (redirect) GET arm.
    #[test]
    fn the_slashless_index_keeps_the_method_semantics() {
        let bare = format!("/d/{TOKEN}");
        for method in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS"] {
            assert_eq!(
                resolve(method, &bare, TOKEN),
                Resolution::MethodNotAllowed,
                "{method} on the bare index must be 405"
            );
        }
        assert_eq!(resolve("HEAD", &bare, TOKEN), Resolution::Redirect);
    }
}
