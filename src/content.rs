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
        let mut r = Response::new(Body::from(html));
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
