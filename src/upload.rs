//! One-time upload URLs. An authenticated owner (MCP `create_upload`, REST `POST /api/uploads`) mints a
//! ticket; anyone holding its URL may `PUT` the raw bytes once within `TICKET_TTL`. The PUT creates the
//! file (or a new version when the ticket names one). Lets agents upload large or binary files straight
//! from a shell (`curl -T`) instead of passing the bytes through a tool call.

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    routing::put,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    app::AppState,
    db,
    error::{ApiResult, AppError},
    ids,
    models::{Access, Owner},
    service::{self, FileMeta, Upload},
};

/// How long an upload URL stays valid.
pub const TICKET_TTL: i64 = 10 * 60;

/// What the eventual PUT turns into. File settings only apply when the ticket creates a file.
#[derive(Default, Serialize, Deserialize, schemars::JsonSchema, Debug)]
pub struct TicketMeta {
    /// Filename including extension (decides the kind: .html .md .pdf .png …). For a new version it
    /// defaults to the previous version's filename.
    pub filename: Option<String>,
    /// MIME type override, e.g. `application/pdf`. Usually inferred from the filename.
    pub mime: Option<String>,
    /// Title shown in the recipient's sidebar (new files only). Defaults to the filename stem.
    pub title: Option<String>,
    /// Readable prefix of the public id (new files only). Defaults to a slug of the title.
    pub slug: Option<String>,
    /// `link` (default), `recipients` or `off` (new files only).
    pub access: Option<Access>,
    /// Email gate (new files only).
    pub email_gate: Option<bool>,
    /// RFC 3339 expiry of the file (new files only).
    pub expires_at: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct UploadTicketJson {
    pub upload_url: String,
    pub method: &'static str,
    pub expires_at: String,
    /// Set when the upload adds a version to this file.
    pub file_id: Option<String>,
    /// Ready-to-run upload command; replace `FILE` with the local path.
    pub curl: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/upload/{token}", put(put_upload))
}

pub async fn create_ticket(
    st: &AppState,
    owner: &Owner,
    file_public_id: Option<&str>,
    meta: TicketMeta,
) -> ApiResult<UploadTicketJson> {
    let file_id = match file_public_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => Some(service::load_file(st, owner.id, id).await?),
        None => None,
    };
    if let Some(e) = meta.expires_at.as_deref().filter(|s| !s.trim().is_empty())
        && db::parse_rfc3339(e.trim()).is_none()
    {
        return Err(AppError::bad(
            "expires_at must be RFC 3339, e.g. 2026-12-31T23:59:59Z",
        ));
    }
    let now = db::now();
    // tickets are worthless once expired; drop old ones whenever new ones are minted
    sqlx::query("DELETE FROM upload_tickets WHERE expires_at < ?")
        .bind(now - 24 * 3600)
        .execute(&st.db)
        .await?;
    let token = ids::opaque_token();
    let expires_at = now + TICKET_TTL;
    sqlx::query(
        "INSERT INTO upload_tickets (token_hash, owner_id, file_id, meta, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(ids::sha256_hex(token.as_bytes()))
    .bind(owner.id)
    .bind(file_id.as_ref().map(|f| f.id))
    .bind(serde_json::to_string(&meta).map_err(|e| AppError::Internal(e.into()))?)
    .bind(now)
    .bind(expires_at)
    .execute(&st.db)
    .await?;
    let upload_url = format!("{}/upload/{token}", st.cfg.public_url);
    Ok(UploadTicketJson {
        curl: format!("curl --fail-with-body -T FILE '{upload_url}'"),
        upload_url,
        method: "PUT",
        expires_at: db::rfc3339(expires_at),
        file_id: file_id.map(|f| f.public_id),
    })
}

#[derive(sqlx::FromRow)]
struct Claimed {
    owner_id: i64,
    file_id: Option<i64>,
    meta: String,
}

/// `PUT /upload/{token}` with the raw bytes as body. The ticket is spent on first use, even if the
/// upload then fails. 201 + file JSON for a new file, 200 for a new version.
async fn put_upload(
    State(st): State<AppState>,
    Path(token): Path<String>,
    req: Request,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let max = st.cfg.max_upload_bytes;
    let declared = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > max) {
        return Err(AppError::TooLarge);
    }
    let now = db::now();
    let ticket = sqlx::query_as::<_, Claimed>(
        "UPDATE upload_tickets SET used_at = ? \
         WHERE token_hash = ? AND used_at IS NULL AND expires_at > ? \
         RETURNING owner_id, file_id, meta",
    )
    .bind(now)
    .bind(ids::sha256_hex(token.as_bytes()))
    .bind(now)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| {
        AppError::bad(
            "upload URL is unknown, already used or expired; mint a new one with create_upload",
        )
    })?;
    let owner = sqlx::query_as::<_, Owner>("SELECT * FROM owners WHERE id = ?")
        .bind(ticket.owner_id)
        .fetch_one(&st.db)
        .await?;
    let meta: TicketMeta =
        serde_json::from_str(&ticket.meta).map_err(|e| AppError::Internal(e.into()))?;

    let mut body = req.into_body().into_data_stream();
    let mut w = st.blobs.writer().await?;
    let mut size = 0usize;
    while let Some(chunk) = body.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                w.abort().await;
                return Err(AppError::bad(format!("upload interrupted: {e}")));
            }
        };
        size += chunk.len();
        if size > max {
            w.abort().await;
            return Err(AppError::TooLarge);
        }
        w.write(&chunk).await?;
    }
    if size == 0 {
        w.abort().await;
        return Err(AppError::bad("empty upload"));
    }
    let up = Upload {
        blob: w.finish().await?,
        mime: meta.mime,
        filename: meta.filename,
    };

    match ticket.file_id {
        Some(file_id) => {
            let public_id: String = sqlx::query_scalar(
                "SELECT public_id FROM files WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(file_id)
            .fetch_optional(&st.db)
            .await?
            .ok_or(AppError::NotFound)?;
            let file = service::add_version(&st, &owner, &public_id, up).await?;
            Ok((StatusCode::OK, Json(json!(file))))
        }
        None => {
            let file_meta = FileMeta {
                title: meta.title,
                slug: meta.slug,
                access: meta.access,
                password: None,
                email_gate: meta.email_gate,
                expires_at: meta.expires_at,
            };
            let file = service::create_file(&st, &owner, file_meta, up).await?;
            Ok((StatusCode::CREATED, Json(json!(file))))
        }
    }
}
