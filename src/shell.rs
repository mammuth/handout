//! Embedded frontend (Vite build in `web/dist`) and the landing page.

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use axum_extra::extract::CookieJar;
use rust_embed::Embed;

use crate::{app::AppState, auth, oauth::SESSION_COOKIE};

#[derive(Embed)]
#[folder = "web/dist"]
#[include = "*.html"]
#[include = "assets/*"]
struct Assets;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(landing))
        .route("/assets/{*path}", get(asset))
        .route("/robots.txt", get(|| async { robots_txt(ROBOTS) }))
        .route("/favicon.svg", get(favicon))
        .route(
            "/favicon.ico",
            get(|| async { Redirect::permanent("/favicon.svg") }),
        )
}

pub fn spa_index() -> Response {
    match Assets::get("index.html") {
        Some(f) => {
            let mut r = Html(f.data.into_owned()).into_response();
            r.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            r
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "frontend not built: run `cd web && npm run build`",
        )
            .into_response(),
    }
}

async fn asset(Path(path): Path<String>) -> Response {
    let Some(f) = Assets::get(&format!("assets/{path}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    let mut r = Response::new(axum::body::Body::from(f.data.into_owned()));
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    r
}

async fn favicon() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../web/public/favicon.svg"),
    )
        .into_response()
}

/// A `robots.txt` body as a response; the content host serves its own through this too.
pub fn robots_txt(body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        body,
    )
        .into_response()
}

/// Owner, auth and agent endpoints are off limits. File links (`/s/…`) stay fetchable on purpose: a
/// crawler only honors `noindex` on pages it was allowed to fetch.
const ROBOTS: &str = "# Nothing here is for search engines; every response carries X-Robots-Tag: noindex.\n\
# File links (/s/...) stay fetchable so crawlers see that header and drop them.\n\
User-agent: *\n\
Disallow: /admin\n\
Disallow: /api/\n\
Disallow: /auth/\n\
Disallow: /login\n\
Disallow: /logout\n\
Disallow: /mcp\n\
Disallow: /oauth/\n\
Disallow: /.well-known/\n";

/// Signed-in owners go straight to the admin; everyone else gets the landing with a sign-in link.
async fn landing(State(st): State<AppState>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(&st.cfg.cookie_name(SESSION_COOKIE))
        && let Ok(Some(_)) = auth::session_owner(&st.db, c.value()).await
    {
        return Redirect::to("/admin").into_response();
    }
    Html(LANDING).into_response()
}

const LANDING: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>handout</title><link rel="icon" type="image/svg+xml" href="/favicon.svg"><style>
:root{--bg:#f6f1e7;--fg:#1c1a16;--muted:#6f685c;--accent:#c2410c}
@media(prefers-color-scheme:dark){:root{--bg:#161411;--fg:#efe9dc;--muted:#9a9184;--accent:#fb923c}}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--fg);font:18px/1.5 ui-sans-serif,system-ui,sans-serif}
main{max-width:34rem;padding:2rem}h1{display:flex;align-items:center;gap:.6rem;font-size:3rem;letter-spacing:-.03em;margin:0 0 .5rem}h1 img{width:3.4rem;height:3.4rem}p{color:var(--muted);margin:.4rem 0}code{color:var(--accent)}
.btn{display:inline-block;margin-top:1.4rem;padding:.65rem 1.2rem;border-radius:999px;background:var(--accent);color:#fff;text-decoration:none;font-weight:600}
.btn:hover{filter:brightness(1.08)}
</style></head><body><main><h1><img src="/favicon.svg" alt="">handout</h1><p>Files shared with you arrive as links. If you have one, open it.</p>
<p>Agents connect at <code>/mcp</code>.</p><a class="btn" href="/admin">Sign in as owner</a></main></body></html>"#;
