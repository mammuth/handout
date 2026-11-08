//! Owner REST API under `/api`. Bearer auth (API key or OAuth access token).

use axum::{
    Json, Router,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{StatusCode, header},
    routing::{delete, get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::{
    app::AppState,
    auth::OwnerAuth,
    db,
    error::{ApiResult, AppError},
    models::Access,
    service::{self, FileMeta, Patch, ShareTarget, Upload},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/files", get(list_files).post(create_file))
        .route(
            "/files/{id}",
            get(get_file).patch(patch_file).delete(delete_file),
        )
        .route("/files/{id}/content", axum::routing::put(put_content))
        .route("/files/{id}/opens", get(file_opens))
        .route("/files/{id}/share", post(share_file))
        .route("/files/{id}/invites/{rid}", delete(revoke_invite))
        .route("/recipients", get(list_recipients).post(create_recipient))
        .route(
            "/recipients/{id}",
            get(get_recipient)
                .patch(patch_recipient)
                .delete(delete_recipient),
        )
        .route("/recipients/{id}/rotate-token", post(rotate_token))
        .route("/opens", get(list_opens))
        .route("/keys", get(list_keys).post(create_key))
        .route("/keys/{id}", delete(revoke_key))
}

/// `null` → Some(None), absent → None, value → Some(Some(v)).
fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(d).map(Some)
}

fn to_patch<T>(v: Option<Option<T>>) -> Option<Patch<T>> {
    match v {
        None => None,
        Some(None) => Some(Patch::Clear),
        Some(Some(x)) => Some(Patch::Set(x)),
    }
}

async fn me(State(st): State<AppState>, OwnerAuth(owner): OwnerAuth) -> Json<Value> {
    Json(json!(service::owner_json(&st, &owner)))
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    limit: Option<i64>,
}

async fn list_files(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let files = service::list_files(&st, &owner, q.q.as_deref(), q.limit.unwrap_or(100)).await?;
    Ok(Json(json!({ "files": files })))
}

#[derive(Deserialize)]
struct JsonUpload {
    #[serde(flatten)]
    meta: FileMeta,
    filename: Option<String>,
    mime: Option<String>,
    content: Option<String>,
    content_base64: Option<String>,
}

/// Accepts `multipart/form-data` (field `file` + text fields) or JSON (`content` / `content_base64`).
pub async fn parse_upload(st: &AppState, req: Request) -> ApiResult<(FileMeta, Upload)> {
    let ct = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ct.starts_with("multipart/form-data") {
        let mut mp = Multipart::from_request(req, st)
            .await
            .map_err(|e| AppError::bad(e.body_text()))?;
        let mut meta = FileMeta::default();
        let mut upload: Option<Upload> = None;
        while let Some(mut field) = mp
            .next_field()
            .await
            .map_err(|e| AppError::bad(e.body_text()))?
        {
            let name = field.name().unwrap_or("").to_string();
            if name == "file" {
                let filename = field.file_name().map(str::to_string);
                let mime = field.content_type().map(str::to_string);
                let mut w = st.blobs.writer().await?;
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => w.write(&chunk).await?,
                        Ok(None) => break,
                        Err(e) => {
                            w.abort().await;
                            return Err(if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                                AppError::TooLarge
                            } else {
                                AppError::bad(e.body_text())
                            });
                        }
                    }
                }
                upload = Some(Upload {
                    blob: w.finish().await?,
                    mime,
                    filename,
                });
            } else {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::bad(e.body_text()))?;
                match name.as_str() {
                    "title" => meta.title = Some(text),
                    "slug" => meta.slug = Some(text),
                    "access" => {
                        meta.access =
                            Some(serde_json::from_value(Value::String(text)).map_err(|_| {
                                AppError::bad("access must be link, recipients or off")
                            })?)
                    }
                    "password" => meta.password = Some(text),
                    "email_gate" => {
                        meta.email_gate = Some(matches!(text.trim(), "1" | "true" | "yes" | "on"))
                    }
                    "expires_at" => meta.expires_at = Some(text),
                    _ => {}
                }
            }
        }
        let upload = upload.ok_or_else(|| AppError::bad("multipart field `file` is required"))?;
        return Ok((meta, upload));
    }
    let Json(body): Json<JsonUpload> = Json::from_request(req, st)
        .await
        .map_err(|e| AppError::bad(e.body_text()))?;
    let bytes = decode_content(body.content, body.content_base64)?;
    if bytes.len() > st.cfg.max_upload_bytes {
        return Err(AppError::TooLarge);
    }
    let blob = st.blobs.put_bytes(&bytes).await?;
    Ok((
        body.meta,
        Upload {
            blob,
            mime: body.mime,
            filename: body.filename,
        },
    ))
}

pub fn decode_content(
    content: Option<String>,
    content_base64: Option<String>,
) -> ApiResult<Vec<u8>> {
    match (content, content_base64) {
        (Some(text), _) => Ok(text.into_bytes()),
        (None, Some(b64)) => {
            let cleaned: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
            STANDARD
                .decode(&cleaned)
                .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&cleaned))
                .map_err(|_| AppError::bad("content_base64 is not valid base64"))
        }
        (None, None) => Err(AppError::bad("content or content_base64 is required")),
    }
}

async fn create_file(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    req: Request,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let (meta, upload) = parse_upload(&st, req).await?;
    let file = service::create_file(&st, &owner, meta, upload).await?;
    Ok((StatusCode::CREATED, Json(json!(file))))
}

async fn put_content(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
    req: Request,
) -> ApiResult<Json<Value>> {
    let (_meta, upload) = parse_upload(&st, req).await?;
    Ok(Json(json!(
        service::add_version(&st, &owner, &id, upload).await?
    )))
}

async fn get_file(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!(service::get_file(&st, &owner, &id).await?)))
}

#[derive(Deserialize, Default)]
struct FilePatchJson {
    title: Option<String>,
    access: Option<Access>,
    #[serde(default, deserialize_with = "double_option")]
    password: Option<Option<String>>,
    email_gate: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    expires_at: Option<Option<String>>,
}

async fn patch_file(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
    Json(body): Json<FilePatchJson>,
) -> ApiResult<Json<Value>> {
    let patch = service::patch_from_strings(
        body.title,
        body.access,
        to_patch(body.password),
        body.email_gate,
        to_patch(body.expires_at),
    )?;
    Ok(Json(json!(
        service::patch_file(&st, &owner, &id, patch).await?
    )))
}

async fn delete_file(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    service::delete_file(&st, &owner, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct OpensQuery {
    file_id: Option<String>,
    recipient_id: Option<String>,
    since: Option<String>,
    limit: Option<i64>,
}

fn parse_since(s: Option<&str>) -> ApiResult<Option<i64>> {
    match s {
        None => Ok(None),
        Some(s) => db::parse_rfc3339(s)
            .map(Some)
            .ok_or_else(|| AppError::bad("since must be RFC 3339")),
    }
}

async fn file_opens(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
    Query(q): Query<OpensQuery>,
) -> ApiResult<Json<Value>> {
    let opens = service::list_opens(
        &st,
        &owner,
        Some(&id),
        q.recipient_id.as_deref(),
        parse_since(q.since.as_deref())?,
        q.limit.unwrap_or(200),
    )
    .await?;
    Ok(Json(json!({ "opens": opens })))
}

async fn list_opens(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Query(q): Query<OpensQuery>,
) -> ApiResult<Json<Value>> {
    let opens = service::list_opens(
        &st,
        &owner,
        q.file_id.as_deref(),
        q.recipient_id.as_deref(),
        parse_since(q.since.as_deref())?,
        q.limit.unwrap_or(200),
    )
    .await?;
    Ok(Json(json!({ "opens": opens })))
}

#[derive(Deserialize)]
struct ShareBody {
    recipient_id: Option<String>,
    name: Option<String>,
    email: Option<String>,
    company: Option<String>,
}

async fn share_file(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
    Json(body): Json<ShareBody>,
) -> ApiResult<Json<Value>> {
    let target = match body.recipient_id {
        Some(rid) => ShareTarget::Id(rid),
        None => ShareTarget::Lookup {
            name: body.name,
            email: body.email,
            company: body.company,
        },
    };
    Ok(Json(json!(
        service::share_file(&st, &owner, &id, target).await?
    )))
}

async fn revoke_invite(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path((id, rid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    service::revoke_invite(&st, &owner, &id, &rid).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_recipients(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let rs = service::find_recipients(&st, &owner, q.q.as_deref(), q.limit.unwrap_or(100)).await?;
    Ok(Json(
        json!({ "recipients": rs.iter().map(service::recipient_json).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
struct RecipientBody {
    name: Option<String>,
    email: Option<String>,
    company: Option<String>,
}

async fn create_recipient(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Json(b): Json<RecipientBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = service::create_recipient(
        &st,
        &owner,
        b.name.as_deref().unwrap_or(""),
        b.email.as_deref(),
        b.company.as_deref(),
    )
    .await?;
    let mut v = json!(service::recipient_json(&r));
    v["token"] = json!(r.token);
    Ok((StatusCode::CREATED, Json(v)))
}

async fn get_recipient(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!(
        service::recipient_detail(&st, &owner, &id).await?
    )))
}

#[derive(Deserialize)]
struct RecipientPatch {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    email: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    company: Option<Option<String>>,
}

async fn patch_recipient(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
    Json(b): Json<RecipientPatch>,
) -> ApiResult<Json<Value>> {
    let r = service::update_recipient(
        &st,
        &owner,
        &id,
        b.name.as_deref(),
        to_patch(b.email),
        to_patch(b.company),
    )
    .await?;
    Ok(Json(json!(service::recipient_json(&r))))
}

async fn rotate_token(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let r = service::rotate_recipient_token(&st, &owner, &id).await?;
    let mut v = json!(service::recipient_json(&r));
    v["token"] = json!(r.token);
    Ok(Json(v))
}

async fn delete_recipient(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    service::delete_recipient(&st, &owner, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct KeyBody {
    name: String,
}

async fn list_keys(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "keys": service::list_api_keys(&st, &owner).await? }),
    ))
}

async fn create_key(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Json(b): Json<KeyBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let (k, raw) = service::create_api_key(&st, &owner, &b.name).await?;
    let mut v = json!(k);
    v["key"] = json!(raw);
    Ok((StatusCode::CREATED, Json(v)))
}

async fn revoke_key(
    State(st): State<AppState>,
    OwnerAuth(owner): OwnerAuth,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    service::revoke_api_key(&st, &owner, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
