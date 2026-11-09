//! Content host: raw bytes of a file version, only with a valid signature minted by the viewer API.

use axum::{
    Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::{
    app::AppState,
    db,
    error::{ApiResult, AppError},
    markdown,
    models::{Access, Kind},
    service, view,
};

/// Activity beacon injected into HTML/Markdown served inline. Runs inside the sandboxed iframe (opaque
/// origin) and only talks to the parent shell via postMessage: input activity + scroll depth, once a second.
pub const BEACON: &str = r#"<script>(function(){var p=window.parent;if(!p||p===window)return;var last=0,maxs=0;function act(){last=Date.now()}
["mousemove","scroll","keydown","pointerdown","touchstart","wheel"].forEach(function(e){addEventListener(e,act,{passive:true,capture:true})});
function depth(){var d=document.documentElement,b=document.body;var h=Math.max(d.scrollHeight||0,b?b.scrollHeight:0);var y=(window.scrollY||d.scrollTop||0)+window.innerHeight;var pct=h>0?Math.min(100,Math.round(100*y/h)):100;if(pct>maxs)maxs=pct;return maxs}
setInterval(function(){p.postMessage({handout:"activity",active:Date.now()-last<10000,scroll:depth()},"*")},1000);addEventListener("load",depth)})();</script>"#;

/// Insert the beacon before `</body>` (case-insensitive), else append.
pub fn inject_beacon(html: &[u8]) -> Vec<u8> {
    let lower = html.to_ascii_lowercase();
    let pos = lower
        .windows(7)
        .rposition(|w| w == b"</body>")
        .unwrap_or(html.len());
    let mut out = Vec::with_capacity(html.len() + BEACON.len());
    out.extend_from_slice(&html[..pos]);
    out.extend_from_slice(BEACON.as_bytes());
    out.extend_from_slice(&html[pos..]);
    out
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{file}", get(serve_inline))
        .route("/{owner}/{file}/download", get(serve_download))
        .fallback(|| async { StatusCode::NOT_FOUND })
}

#[derive(Deserialize)]
struct ContentQuery {
    t: String,
    v: Option<i64>,
}

async fn serve_inline(
    State(st): State<AppState>,
    Path((owner, file)): Path<(String, String)>,
    Query(q): Query<ContentQuery>,
) -> ApiResult<Response> {
    serve(&st, &owner, &file, q, false).await
}

async fn serve_download(
    State(st): State<AppState>,
    Path((owner, file)): Path<(String, String)>,
    Query(q): Query<ContentQuery>,
) -> ApiResult<Response> {
    serve(&st, &owner, &file, q, true).await
}

async fn serve(
    st: &AppState,
    owner_slug: &str,
    public_id: &str,
    q: ContentQuery,
    download: bool,
) -> ApiResult<Response> {
    let owner = service::owner_by_slug(&st.db, owner_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let file = service::load_file(st, owner.id, public_id).await?;
    let version = match q.v {
        Some(n) => service::version(st, file.id, n).await?,
        None => service::latest_version(st, file.id).await?,
    };
    if !view::verify_content_sig(st, owner_slug, public_id, version.number, &q.t) {
        return Err(AppError::Forbidden);
    }
    if file.access() == Access::Off || file.expires_at.is_some_and(|e| e <= db::now()) {
        return Err(AppError::Forbidden);
    }
    let kind = file.kind();
    let disposition = format!(
        "{}; filename=\"{}\"",
        if download { "attachment" } else { "inline" },
        version.filename.replace(['"', '\\', '\r', '\n'], "_")
    );

    let mut resp = if kind == Kind::Markdown && !download {
        let bytes = st.blobs.read(&version.blob_sha256).await?;
        let html = markdown::render_document(&String::from_utf8_lossy(&bytes), &file.title);
        let mut r = Response::new(Body::from(inject_beacon(html.as_bytes())));
        r.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        r
    } else if kind == Kind::Html && !download {
        let bytes = st.blobs.read(&version.blob_sha256).await?;
        let body = inject_beacon(&bytes);
        let mut r = Response::new(Body::from(body));
        r.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        r
    } else {
        let f = st
            .blobs
            .open(&version.blob_sha256)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut r = Response::new(Body::from_stream(ReaderStream::new(f)));
        let ct = match kind {
            Kind::Html => "text/html; charset=utf-8".to_string(),
            Kind::Markdown => "text/markdown; charset=utf-8".to_string(),
            _ => version.mime.clone(),
        };
        r.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&ct)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );
        r.headers_mut().insert(
            header::CONTENT_LENGTH,
            HeaderValue::from(version.size as u64),
        );
        r
    };

    let h = resp.headers_mut();
    let frame_ancestors = format!("frame-ancestors {}", st.cfg.public_url);
    let csp = match kind {
        // scripts allowed but on an opaque origin; also applies when opened top-level
        Kind::Html | Kind::Markdown | Kind::Other => {
            format!(
                "sandbox allow-scripts allow-forms allow-popups allow-downloads allow-modals; {frame_ancestors}"
            )
        }
        // browser PDF viewers and images do not need (and some dislike) sandboxing
        Kind::Pdf | Kind::Image => frame_ancestors,
    };
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).unwrap(),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).unwrap_or(HeaderValue::from_static("inline")),
    );
    Ok(resp.into_response())
}
