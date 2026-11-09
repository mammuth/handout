//! Business logic shared by the REST API and the MCP tools. Everything here is owner-scoped.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    app::AppState,
    auth,
    blobs::StoredBlob,
    db::{self, rfc3339},
    error::{ApiResult, AppError},
    ids,
    models::{Access, ApiKey, FileRow, FileVersion, Kind, Owner, Recipient},
};

// ---------- output DTOs ----------

#[derive(Serialize, Clone, Debug)]
pub struct OwnerJson {
    pub slug: String,
    pub name: String,
    pub public_url: String,
    pub mcp_url: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct FileSummary {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    pub access: Access,
    pub has_password: bool,
    pub email_gate: bool,
    pub expires_at: Option<String>,
    pub version: i64,
    pub filename: String,
    pub size: i64,
    pub mime: String,
    pub created_at: String,
    pub updated_at: String,
    pub file_link: String,
}

#[derive(Serialize, Debug)]
pub struct VersionJson {
    pub number: i64,
    pub size: i64,
    pub mime: String,
    pub filename: String,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RecipientJson {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub company: Option<String>,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct InviteJson {
    pub recipient: RecipientJson,
    pub created_at: String,
    pub last_opened_at: Option<String>,
    pub recipient_link: String,
    pub opens: i64,
    pub active_seconds: i64,
    pub max_scroll_pct: i64,
}

#[derive(Serialize, Debug)]
pub struct FileDetail {
    #[serde(flatten)]
    pub summary: FileSummary,
    pub versions: Vec<VersionJson>,
    pub invites: Vec<InviteJson>,
    pub opens: i64,
    /// Sum over all opens: seconds the tab was visible / the viewer was actively engaged.
    pub visible_seconds: i64,
    pub active_seconds: i64,
}

#[derive(Serialize, Debug)]
pub struct RecipientFile {
    #[serde(flatten)]
    pub file: FileSummary,
    pub invited_at: String,
    pub last_opened_at: Option<String>,
    pub recipient_link: String,
    pub opens: i64,
    pub active_seconds: i64,
    pub max_scroll_pct: i64,
}

#[derive(Serialize, Debug)]
pub struct RecipientDetail {
    #[serde(flatten)]
    pub recipient: RecipientJson,
    pub token: String,
    pub files: Vec<RecipientFile>,
}

#[derive(Serialize, Debug)]
pub struct RecipientRef {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Debug)]
pub struct OpenJson {
    pub file_id: String,
    pub title: String,
    pub version: i64,
    pub recipient: Option<RecipientRef>,
    pub at: String,
    pub user_agent: String,
    pub visible_seconds: i64,
    pub active_seconds: i64,
    pub max_scroll_pct: i64,
}

#[derive(Serialize, Debug)]
pub struct ShareResult {
    pub file_id: String,
    pub recipient: RecipientJson,
    pub recipient_link: String,
    pub invite_created: bool,
    pub recipient_created: bool,
}

#[derive(Serialize, Debug)]
pub struct ApiKeyJson {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

// ---------- inputs ----------

/// Metadata accepted when creating a file (REST multipart/JSON and MCP share this).
#[derive(Default, Deserialize, schemars::JsonSchema, Debug)]
pub struct FileMeta {
    /// Human title shown in the sidebar. Defaults to the filename stem.
    pub title: Option<String>,
    /// Readable prefix of the public id. Defaults to a slug of the title.
    pub slug: Option<String>,
    /// `link` (default), `recipients` or `off`.
    pub access: Option<Access>,
    /// Password gate.
    pub password: Option<String>,
    /// Email gate: viewer must enter an email, which becomes a recipient.
    pub email_gate: Option<bool>,
    /// RFC 3339 expiry, e.g. `2026-12-31T23:59:59Z`.
    pub expires_at: Option<String>,
}

pub struct Upload {
    pub blob: StoredBlob,
    pub mime: Option<String>,
    pub filename: Option<String>,
}

pub enum Patch<T> {
    Keep,
    Clear,
    Set(T),
}

#[derive(Default)]
pub struct FilePatch {
    pub title: Option<String>,
    pub access: Option<Access>,
    pub password: Option<Patch<String>>,
    pub email_gate: Option<bool>,
    pub expires_at: Option<Patch<i64>>,
}

pub enum ShareTarget {
    Id(String),
    Lookup {
        name: Option<String>,
        email: Option<String>,
        company: Option<String>,
    },
}

// ---------- owners ----------

pub async fn create_owner(db: &db::Db, slug: &str, name: &str, password: &str) -> ApiResult<Owner> {
    let slug = slug.trim().to_ascii_lowercase();
    let ok = slug.len() >= 2
        && slug.len() <= 32
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-');
    if !ok {
        return Err(AppError::bad("owner slug must be 2-32 chars of [a-z0-9-]"));
    }
    if password.len() < 8 {
        return Err(AppError::bad("password must be at least 8 chars"));
    }
    let hash = auth::hash_password(password)?;
    let now = db::now();
    let id = sqlx::query(
        "INSERT INTO owners (slug, name, password_hash, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&slug)
    .bind(name.trim())
    .bind(&hash)
    .bind(now)
    .execute(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AppError::conflict("owner slug already exists", json!({}))
        }
        other => AppError::from(other),
    })?
    .last_insert_rowid();
    Ok(Owner {
        id,
        slug,
        name: name.trim().to_string(),
        password_hash: hash,
        created_at: now,
    })
}

pub async fn owner_by_slug(db: &db::Db, slug: &str) -> ApiResult<Option<Owner>> {
    Ok(
        sqlx::query_as::<_, Owner>("SELECT * FROM owners WHERE slug = ?")
            .bind(slug)
            .fetch_optional(db)
            .await?,
    )
}

pub fn owner_json(st: &AppState, owner: &Owner) -> OwnerJson {
    OwnerJson {
        slug: owner.slug.clone(),
        name: owner.name.clone(),
        public_url: st.cfg.public_url.clone(),
        mcp_url: format!("{}/mcp", st.cfg.public_url),
    }
}

// ---------- api keys ----------

pub async fn create_api_key(
    st: &AppState,
    owner: &Owner,
    name: &str,
) -> ApiResult<(ApiKeyJson, String)> {
    let raw = ids::api_key();
    let now = db::now();
    let prefix = raw[..10].to_string();
    let id = sqlx::query("INSERT INTO api_keys (owner_id, name, key_hash, key_prefix, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(owner.id)
        .bind(name.trim())
        .bind(ids::sha256_hex(raw.as_bytes()))
        .bind(&prefix)
        .bind(now)
        .execute(&st.db)
        .await?
        .last_insert_rowid();
    Ok((
        ApiKeyJson {
            id,
            name: name.trim().to_string(),
            prefix,
            created_at: rfc3339(now),
            last_used_at: None,
            revoked_at: None,
        },
        raw,
    ))
}

pub async fn list_api_keys(st: &AppState, owner: &Owner) -> ApiResult<Vec<ApiKeyJson>> {
    let rows = sqlx::query_as::<_, ApiKey>(
        "SELECT id, owner_id, name, key_prefix, created_at, last_used_at, revoked_at FROM api_keys WHERE owner_id = ? ORDER BY id",
    )
    .bind(owner.id)
    .fetch_all(&st.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|k| ApiKeyJson {
            id: k.id,
            name: k.name,
            prefix: k.key_prefix,
            created_at: rfc3339(k.created_at),
            last_used_at: k.last_used_at.map(rfc3339),
            revoked_at: k.revoked_at.map(rfc3339),
        })
        .collect())
}

pub async fn revoke_api_key(st: &AppState, owner: &Owner, id: i64) -> ApiResult<()> {
    let n = sqlx::query(
        "UPDATE api_keys SET revoked_at = ? WHERE id = ? AND owner_id = ? AND revoked_at IS NULL",
    )
    .bind(db::now())
    .bind(id)
    .bind(owner.id)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

// ---------- files ----------

pub async fn load_file(st: &AppState, owner_id: i64, public_id: &str) -> ApiResult<FileRow> {
    sqlx::query_as::<_, FileRow>(
        "SELECT * FROM files WHERE owner_id = ? AND public_id = ? AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .bind(public_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn latest_version(st: &AppState, file_id: i64) -> ApiResult<FileVersion> {
    sqlx::query_as::<_, FileVersion>(
        "SELECT * FROM file_versions WHERE file_id = ? ORDER BY number DESC LIMIT 1",
    )
    .bind(file_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn version(st: &AppState, file_id: i64, number: i64) -> ApiResult<FileVersion> {
    sqlx::query_as::<_, FileVersion>("SELECT * FROM file_versions WHERE file_id = ? AND number = ?")
        .bind(file_id)
        .bind(number)
        .fetch_optional(&st.db)
        .await?
        .ok_or(AppError::NotFound)
}

pub fn summary(st: &AppState, owner_slug: &str, f: &FileRow, v: &FileVersion) -> FileSummary {
    FileSummary {
        id: f.public_id.clone(),
        title: f.title.clone(),
        kind: f.kind(),
        access: f.access(),
        has_password: f.password_hash.is_some(),
        email_gate: f.email_gate,
        expires_at: f.expires_at.map(rfc3339),
        version: v.number,
        filename: v.filename.clone(),
        size: v.size,
        mime: v.mime.clone(),
        created_at: rfc3339(f.created_at),
        updated_at: rfc3339(f.updated_at),
        file_link: st.cfg.file_link(owner_slug, &f.public_id),
    }
}

fn parse_expiry(s: &str) -> ApiResult<i64> {
    db::parse_rfc3339(s.trim())
        .ok_or_else(|| AppError::bad("expires_at must be RFC 3339, e.g. 2026-12-31T23:59:59Z"))
}

/// Resolve kind/mime/filename for an upload.
fn classify(up: &Upload, fallback_name: &str) -> (Kind, String, String) {
    let filename = up
        .filename
        .as_deref()
        .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).trim().to_string())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());
    let declared = up
        .mime
        .as_deref()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty() && !m.starts_with("application/octet-stream"));
    let kind = Kind::detect(declared.as_deref().unwrap_or(""), &filename);
    let mime = match (&declared, kind) {
        (Some(m), _) if !m.starts_with("text/plain") => m.clone(),
        _ => kind.default_mime(&filename),
    };
    (kind, mime, filename)
}

pub async fn create_file(
    st: &AppState,
    owner: &Owner,
    meta: FileMeta,
    up: Upload,
) -> ApiResult<FileDetail> {
    let title_hint = meta.title.clone().filter(|t| !t.trim().is_empty());
    let fallback_name = format!(
        "{}.html",
        title_hint
            .as_deref()
            .map(ids::slugify)
            .unwrap_or_else(|| "file".into())
    );
    let (kind, mime, filename) = classify(&up, &fallback_name);
    let title = title_hint.unwrap_or_else(|| {
        let stem = filename
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(&filename);
        stem.to_string()
    });
    let slug = ids::slugify(
        meta.slug
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&title),
    );
    let expires_at = meta
        .expires_at
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(parse_expiry)
        .transpose()?;
    let password_hash = match meta.password.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => Some(auth::hash_password(p)?),
        None => None,
    };
    let access = meta.access.unwrap_or(Access::Link);
    let now = db::now();

    let mut tx = st.db.begin().await?;
    let mut public_id = ids::file_public_id(&slug);
    let file_id = loop {
        let res = sqlx::query(
            "INSERT INTO files (owner_id, public_id, slug, title, kind, access, password_hash, email_gate, expires_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(owner.id)
        .bind(&public_id)
        .bind(&slug)
        .bind(title.trim())
        .bind(kind.as_str())
        .bind(access.as_str())
        .bind(&password_hash)
        .bind(meta.email_gate.unwrap_or(false))
        .bind(expires_at)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await;
        match res {
            Ok(r) => break r.last_insert_rowid(),
            Err(sqlx::Error::Database(d)) if d.is_unique_violation() => {
                public_id = ids::file_public_id(&slug)
            }
            Err(e) => return Err(e.into()),
        }
    };
    sqlx::query(
        "INSERT INTO file_versions (file_id, number, blob_sha256, size, mime, filename, created_at) VALUES (?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(file_id)
    .bind(&up.blob.sha256)
    .bind(up.blob.size)
    .bind(&mime)
    .bind(&filename)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get_file(st, owner, &public_id).await
}

pub async fn add_version(
    st: &AppState,
    owner: &Owner,
    public_id: &str,
    up: Upload,
) -> ApiResult<FileDetail> {
    let file = load_file(st, owner.id, public_id).await?;
    let prev = latest_version(st, file.id).await?;
    let (kind, mime, filename) = classify(&up, &prev.filename);
    let now = db::now();
    let mut tx = st.db.begin().await?;
    sqlx::query(
        "INSERT INTO file_versions (file_id, number, blob_sha256, size, mime, filename, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(file.id)
    .bind(prev.number + 1)
    .bind(&up.blob.sha256)
    .bind(up.blob.size)
    .bind(&mime)
    .bind(&filename)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE files SET kind = ?, updated_at = ? WHERE id = ?")
        .bind(kind.as_str())
        .bind(now)
        .bind(file.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    get_file(st, owner, public_id).await
}

pub async fn patch_file(
    st: &AppState,
    owner: &Owner,
    public_id: &str,
    patch: FilePatch,
) -> ApiResult<FileDetail> {
    let file = load_file(st, owner.id, public_id).await?;
    let access = patch.access.unwrap_or(file.access());
    let title = patch
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or(file.title);
    let password_hash = match patch.password {
        None | Some(Patch::Keep) => file.password_hash,
        Some(Patch::Clear) => None,
        Some(Patch::Set(p)) if p.is_empty() => None,
        Some(Patch::Set(p)) => Some(auth::hash_password(&p)?),
    };
    let email_gate = patch.email_gate.unwrap_or(file.email_gate);
    let expires_at = match patch.expires_at {
        None | Some(Patch::Keep) => file.expires_at,
        Some(Patch::Clear) => None,
        Some(Patch::Set(ts)) => Some(ts),
    };
    sqlx::query(
        "UPDATE files SET title = ?, access = ?, password_hash = ?, email_gate = ?, expires_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&title)
    .bind(access.as_str())
    .bind(&password_hash)
    .bind(email_gate)
    .bind(expires_at)
    .bind(db::now())
    .bind(file.id)
    .execute(&st.db)
    .await?;
    get_file(st, owner, public_id).await
}

pub fn patch_from_strings(
    title: Option<String>,
    access: Option<Access>,
    password: Option<Patch<String>>,
    email_gate: Option<bool>,
    expires_at: Option<Patch<String>>,
) -> ApiResult<FilePatch> {
    let expires_at = match expires_at {
        None => None,
        Some(Patch::Keep) => Some(Patch::Keep),
        Some(Patch::Clear) => Some(Patch::Clear),
        Some(Patch::Set(s)) if s.trim().is_empty() => Some(Patch::Clear),
        Some(Patch::Set(s)) => Some(Patch::Set(parse_expiry(&s)?)),
    };
    Ok(FilePatch {
        title,
        access,
        password,
        email_gate,
        expires_at,
    })
}

pub async fn get_file(st: &AppState, owner: &Owner, public_id: &str) -> ApiResult<FileDetail> {
    let file = load_file(st, owner.id, public_id).await?;
    let latest = latest_version(st, file.id).await?;
    let versions = sqlx::query_as::<_, FileVersion>(
        "SELECT * FROM file_versions WHERE file_id = ? ORDER BY number DESC",
    )
    .bind(file.id)
    .fetch_all(&st.db)
    .await?;
    let invites = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, i64, Option<i64>, i64, Option<i64>, i64, i64, i64)>(
        "SELECT r.id, r.name, r.email, r.company, r.token, r.created_at, r.last_seen_at, i.created_at, i.last_opened_at, \
                (SELECT COUNT(*) FROM opens o WHERE o.file_id = i.file_id AND o.recipient_id = r.id), \
                (SELECT COALESCE(SUM(o.active_seconds), 0) FROM opens o WHERE o.file_id = i.file_id AND o.recipient_id = r.id), \
                (SELECT COALESCE(MAX(o.max_scroll_pct), 0) FROM opens o WHERE o.file_id = i.file_id AND o.recipient_id = r.id) \
         FROM invites i JOIN recipients r ON r.id = i.recipient_id WHERE i.file_id = ? ORDER BY i.created_at DESC",
    )
    .bind(file.id)
    .fetch_all(&st.db)
    .await?;
    let (opens, visible_seconds, active_seconds): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(visible_seconds), 0), COALESCE(SUM(active_seconds), 0) FROM opens WHERE file_id = ?",
    )
    .bind(file.id)
    .fetch_one(&st.db)
    .await?;
    Ok(FileDetail {
        summary: summary(st, &owner.slug, &file, &latest),
        versions: versions
            .into_iter()
            .map(|v| VersionJson {
                number: v.number,
                size: v.size,
                mime: v.mime,
                filename: v.filename,
                created_at: rfc3339(v.created_at),
            })
            .collect(),
        invites: invites
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    email,
                    company,
                    token,
                    rc,
                    rs,
                    ic,
                    lo,
                    opens,
                    active_seconds,
                    max_scroll_pct,
                )| InviteJson {
                    recipient: RecipientJson {
                        id,
                        name,
                        email,
                        company,
                        created_at: rfc3339(rc),
                        last_seen_at: rs.map(rfc3339),
                    },
                    created_at: rfc3339(ic),
                    last_opened_at: lo.map(rfc3339),
                    recipient_link: st.cfg.recipient_link(&owner.slug, &file.public_id, &token),
                    opens,
                    active_seconds,
                    max_scroll_pct,
                },
            )
            .collect(),
        opens,
        visible_seconds,
        active_seconds,
    })
}

pub async fn list_files(
    st: &AppState,
    owner: &Owner,
    q: Option<&str>,
    limit: i64,
) -> ApiResult<Vec<FileSummary>> {
    let pattern = q
        .map(|q| format!("%{}%", q.trim().to_lowercase()))
        .filter(|p| p.len() > 2);
    let rows = sqlx::query_as::<_, FileRow>(
        "SELECT * FROM files WHERE owner_id = ? AND deleted_at IS NULL \
         AND (? IS NULL OR lower(title) LIKE ? OR public_id LIKE ?) \
         ORDER BY updated_at DESC LIMIT ?",
    )
    .bind(owner.id)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit.clamp(1, 500))
    .fetch_all(&st.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for f in rows {
        let v = latest_version(st, f.id).await?;
        out.push(summary(st, &owner.slug, &f, &v));
    }
    Ok(out)
}

/// Soft-deletes the row (its opens stay in the activity history) and drops the bytes: every blob only
/// this file's versions referenced is removed right away.
pub async fn delete_file(st: &AppState, owner: &Owner, public_id: &str) -> ApiResult<()> {
    let file = load_file(st, owner.id, public_id).await?;
    sqlx::query("UPDATE files SET deleted_at = ?, access = 'off', updated_at = ? WHERE id = ?")
        .bind(db::now())
        .bind(db::now())
        .bind(file.id)
        .execute(&st.db)
        .await?;
    let blobs: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT blob_sha256 FROM file_versions WHERE file_id = ?")
            .bind(file.id)
            .fetch_all(&st.db)
            .await?;
    gc_blobs(st, blobs).await?;
    Ok(())
}

// ---------- blob garbage collection ----------

/// Blobs younger than this are never swept: an upload writes its blob before the version row.
pub const BLOB_SWEEP_GRACE: Duration = Duration::from_secs(3600);
const BLOB_SWEEP_EVERY: Duration = Duration::from_secs(24 * 3600);

/// Remove each candidate blob that no version of a live (non-deleted) file references; returns how
/// many went. A byte-identical upload racing the delete of the only other holder can lose its blob —
/// accepted for a single-binary tool.
pub async fn gc_blobs(st: &AppState, candidates: Vec<String>) -> ApiResult<usize> {
    let mut removed = 0;
    for sha in candidates {
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM file_versions v JOIN files f ON f.id = v.file_id \
             WHERE v.blob_sha256 = ? AND f.deleted_at IS NULL",
        )
        .bind(&sha)
        .fetch_one(&st.db)
        .await?;
        if live == 0 {
            st.blobs.remove(&sha).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Orphan sweep: blobs left by aborted uploads, crashes or cascading owner deletes, plus stale temp
/// files. Only looks at blobs older than `grace`. Returns the number of blobs removed.
pub async fn sweep_blobs(st: &AppState, grace: Duration) -> ApiResult<usize> {
    let cutoff = SystemTime::now() - grace;
    st.blobs.prune_tmp(cutoff).await?;
    let candidates = st.blobs.stale(cutoff).await?;
    gc_blobs(st, candidates).await
}

/// Sweeps at startup and then once a day. Spawned by `serve`.
pub async fn blob_sweep_loop(st: AppState) {
    let mut tick = tokio::time::interval(BLOB_SWEEP_EVERY);
    loop {
        tick.tick().await;
        match sweep_blobs(&st, BLOB_SWEEP_GRACE).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(removed = n, "blob sweep"),
            Err(e) => tracing::warn!(error = %e, "blob sweep failed"),
        }
    }
}

// ---------- recipients ----------

pub fn recipient_json(r: &Recipient) -> RecipientJson {
    RecipientJson {
        id: r.id.clone(),
        name: r.name.clone(),
        email: r.email.clone(),
        company: r.company.clone(),
        created_at: rfc3339(r.created_at),
        last_seen_at: r.last_seen_at.map(rfc3339),
    }
}

fn norm_email(e: Option<&str>) -> ApiResult<Option<String>> {
    match e
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
    {
        None => Ok(None),
        Some(e) if e.contains('@') && !e.contains(char::is_whitespace) => Ok(Some(e)),
        Some(_) => Err(AppError::bad("invalid email")),
    }
}

fn norm_opt(s: Option<&str>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub async fn load_recipient(st: &AppState, owner_id: i64, id: &str) -> ApiResult<Recipient> {
    sqlx::query_as::<_, Recipient>("SELECT * FROM recipients WHERE owner_id = ? AND id = ?")
        .bind(owner_id)
        .bind(id)
        .fetch_optional(&st.db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn recipient_by_token(
    st: &AppState,
    owner_id: i64,
    token: &str,
) -> ApiResult<Option<Recipient>> {
    Ok(
        sqlx::query_as::<_, Recipient>("SELECT * FROM recipients WHERE owner_id = ? AND token = ?")
            .bind(owner_id)
            .bind(token)
            .fetch_optional(&st.db)
            .await?,
    )
}

pub async fn recipient_by_email(
    st: &AppState,
    owner_id: i64,
    email: &str,
) -> ApiResult<Option<Recipient>> {
    Ok(
        sqlx::query_as::<_, Recipient>("SELECT * FROM recipients WHERE owner_id = ? AND email = ?")
            .bind(owner_id)
            .bind(email)
            .fetch_optional(&st.db)
            .await?,
    )
}

pub async fn create_recipient(
    st: &AppState,
    owner: &Owner,
    name: &str,
    email: Option<&str>,
    company: Option<&str>,
) -> ApiResult<Recipient> {
    let email = norm_email(email)?;
    let name = norm_opt(Some(name))
        .or_else(|| {
            email
                .as_deref()
                .and_then(|e| e.split('@').next())
                .map(str::to_string)
        })
        .ok_or_else(|| AppError::bad("name or email required"))?;
    if let Some(e) = &email
        && let Some(existing) = recipient_by_email(st, owner.id, e).await?
    {
        return Err(AppError::conflict(
            "a recipient with this email already exists",
            json!({ "recipient": recipient_json(&existing) }),
        ));
    }
    let r = Recipient {
        id: uuid::Uuid::new_v4().to_string(),
        owner_id: owner.id,
        name,
        email,
        company: norm_opt(company),
        token: ids::recipient_token(),
        created_at: db::now(),
        last_seen_at: None,
    };
    sqlx::query("INSERT INTO recipients (id, owner_id, name, email, company, token, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&r.id)
        .bind(r.owner_id)
        .bind(&r.name)
        .bind(&r.email)
        .bind(&r.company)
        .bind(&r.token)
        .bind(r.created_at)
        .execute(&st.db)
        .await?;
    Ok(r)
}

pub async fn update_recipient(
    st: &AppState,
    owner: &Owner,
    id: &str,
    name: Option<&str>,
    email: Option<Patch<String>>,
    company: Option<Patch<String>>,
) -> ApiResult<Recipient> {
    let r = load_recipient(st, owner.id, id).await?;
    let name = norm_opt(name).unwrap_or(r.name);
    let email = match email {
        None | Some(Patch::Keep) => r.email,
        Some(Patch::Clear) => None,
        Some(Patch::Set(e)) => norm_email(Some(&e))?,
    };
    let company = match company {
        None | Some(Patch::Keep) => r.company,
        Some(Patch::Clear) => None,
        Some(Patch::Set(c)) => norm_opt(Some(&c)),
    };
    sqlx::query("UPDATE recipients SET name = ?, email = ?, company = ? WHERE id = ?")
        .bind(&name)
        .bind(&email)
        .bind(&company)
        .bind(&r.id)
        .execute(&st.db)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                AppError::conflict("another recipient has this email", json!({}))
            }
            other => other.into(),
        })?;
    load_recipient(st, owner.id, id).await
}

pub async fn rotate_recipient_token(
    st: &AppState,
    owner: &Owner,
    id: &str,
) -> ApiResult<Recipient> {
    let r = load_recipient(st, owner.id, id).await?;
    sqlx::query("UPDATE recipients SET token = ? WHERE id = ?")
        .bind(ids::recipient_token())
        .bind(&r.id)
        .execute(&st.db)
        .await?;
    load_recipient(st, owner.id, id).await
}

pub async fn delete_recipient(st: &AppState, owner: &Owner, id: &str) -> ApiResult<()> {
    let r = load_recipient(st, owner.id, id).await?;
    sqlx::query("DELETE FROM recipients WHERE id = ?")
        .bind(&r.id)
        .execute(&st.db)
        .await?;
    Ok(())
}

pub async fn find_recipients(
    st: &AppState,
    owner: &Owner,
    q: Option<&str>,
    limit: i64,
) -> ApiResult<Vec<Recipient>> {
    let pattern = q
        .map(|q| format!("%{}%", q.trim().to_lowercase()))
        .filter(|p| p.len() > 2);
    Ok(sqlx::query_as::<_, Recipient>(
        "SELECT * FROM recipients WHERE owner_id = ? \
         AND (? IS NULL OR lower(name) LIKE ? OR lower(email) LIKE ? OR lower(company) LIKE ?) \
         ORDER BY name COLLATE NOCASE LIMIT ?",
    )
    .bind(owner.id)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit.clamp(1, 500))
    .fetch_all(&st.db)
    .await?)
}

pub async fn recipient_detail(
    st: &AppState,
    owner: &Owner,
    id: &str,
) -> ApiResult<RecipientDetail> {
    let r = load_recipient(st, owner.id, id).await?;
    let rows = sqlx::query_as::<_, (i64, i64, Option<i64>, i64, i64, i64)>(
        "SELECT i.file_id, i.created_at, i.last_opened_at, \
                (SELECT COUNT(*) FROM opens o WHERE o.file_id = i.file_id AND o.recipient_id = i.recipient_id), \
                (SELECT COALESCE(SUM(o.active_seconds), 0) FROM opens o WHERE o.file_id = i.file_id AND o.recipient_id = i.recipient_id), \
                (SELECT COALESCE(MAX(o.max_scroll_pct), 0) FROM opens o WHERE o.file_id = i.file_id AND o.recipient_id = i.recipient_id) \
         FROM invites i JOIN files f ON f.id = i.file_id \
         WHERE i.recipient_id = ? AND f.deleted_at IS NULL ORDER BY i.created_at DESC",
    )
    .bind(&r.id)
    .fetch_all(&st.db)
    .await?;
    let mut files = Vec::with_capacity(rows.len());
    for (file_id, invited_at, last_opened_at, opens, active_seconds, max_scroll_pct) in rows {
        let f = sqlx::query_as::<_, FileRow>("SELECT * FROM files WHERE id = ?")
            .bind(file_id)
            .fetch_one(&st.db)
            .await?;
        let v = latest_version(st, f.id).await?;
        files.push(RecipientFile {
            recipient_link: st.cfg.recipient_link(&owner.slug, &f.public_id, &r.token),
            file: summary(st, &owner.slug, &f, &v),
            invited_at: rfc3339(invited_at),
            last_opened_at: last_opened_at.map(rfc3339),
            opens,
            active_seconds,
            max_scroll_pct,
        });
    }
    Ok(RecipientDetail {
        recipient: recipient_json(&r),
        token: r.token.clone(),
        files,
    })
}

// ---------- invites ----------

async fn resolve_share_target(
    st: &AppState,
    owner: &Owner,
    target: ShareTarget,
) -> ApiResult<(Recipient, bool)> {
    match target {
        ShareTarget::Id(id) => Ok((load_recipient(st, owner.id, &id).await?, false)),
        ShareTarget::Lookup {
            name,
            email,
            company,
        } => {
            let email_n = norm_email(email.as_deref())?;
            if let Some(e) = &email_n
                && let Some(r) = recipient_by_email(st, owner.id, e).await?
            {
                return Ok((r, false));
            }
            let name_n = norm_opt(name.as_deref());
            if email_n.is_none() {
                let Some(n) = &name_n else {
                    return Err(AppError::bad("recipient_id, email or name required"));
                };
                let matches = sqlx::query_as::<_, Recipient>(
                    "SELECT * FROM recipients WHERE owner_id = ? AND lower(name) = lower(?) ORDER BY created_at",
                )
                .bind(owner.id)
                .bind(n)
                .fetch_all(&st.db)
                .await?;
                match matches.len() {
                    1 => return Ok((matches.into_iter().next().unwrap(), false)),
                    0 => {}
                    _ => {
                        return Err(AppError::conflict(
                            "several recipients match this name; pass recipient_id or email",
                            json!({ "candidates": matches.iter().map(recipient_json).collect::<Vec<_>>() }),
                        ));
                    }
                }
            }
            let r = create_recipient(
                st,
                owner,
                name_n.as_deref().unwrap_or(""),
                email_n.as_deref(),
                company.as_deref(),
            )
            .await?;
            Ok((r, true))
        }
    }
}

pub async fn share_file(
    st: &AppState,
    owner: &Owner,
    public_id: &str,
    target: ShareTarget,
) -> ApiResult<ShareResult> {
    let file = load_file(st, owner.id, public_id).await?;
    let (recipient, recipient_created) = resolve_share_target(st, owner, target).await?;
    let n = sqlx::query(
        "INSERT OR IGNORE INTO invites (file_id, recipient_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(file.id)
    .bind(&recipient.id)
    .bind(db::now())
    .execute(&st.db)
    .await?
    .rows_affected();
    Ok(ShareResult {
        file_id: file.public_id.clone(),
        recipient_link: st
            .cfg
            .recipient_link(&owner.slug, &file.public_id, &recipient.token),
        recipient: recipient_json(&recipient),
        invite_created: n > 0,
        recipient_created,
    })
}

pub async fn revoke_invite(
    st: &AppState,
    owner: &Owner,
    public_id: &str,
    recipient_id: &str,
) -> ApiResult<()> {
    let file = load_file(st, owner.id, public_id).await?;
    let n = sqlx::query("DELETE FROM invites WHERE file_id = ? AND recipient_id = ?")
        .bind(file.id)
        .bind(recipient_id)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

// ---------- opens ----------

pub async fn list_opens(
    st: &AppState,
    owner: &Owner,
    file_public_id: Option<&str>,
    recipient_id: Option<&str>,
    since: Option<i64>,
    limit: i64,
) -> ApiResult<Vec<OpenJson>> {
    let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>, i64, String, i64, i64, i64)>(
        "SELECT f.public_id, f.title, o.version_number, o.recipient_id, r.name, o.created_at, o.user_agent, \
                o.visible_seconds, o.active_seconds, o.max_scroll_pct \
         FROM opens o JOIN files f ON f.id = o.file_id LEFT JOIN recipients r ON r.id = o.recipient_id \
         WHERE f.owner_id = ? AND (? IS NULL OR f.public_id = ?) AND (? IS NULL OR o.recipient_id = ?) \
         AND (? IS NULL OR o.created_at >= ?) ORDER BY o.created_at DESC LIMIT ?",
    )
    .bind(owner.id)
    .bind(file_public_id)
    .bind(file_public_id)
    .bind(recipient_id)
    .bind(recipient_id)
    .bind(since)
    .bind(since)
    .bind(limit.clamp(1, 1000))
    .fetch_all(&st.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                file_id,
                title,
                version,
                rid,
                rname,
                at,
                ua,
                visible_seconds,
                active_seconds,
                max_scroll_pct,
            )| OpenJson {
                file_id,
                title,
                version,
                recipient: rid.map(|id| RecipientRef {
                    id,
                    name: rname.unwrap_or_default(),
                }),
                at: rfc3339(at),
                user_agent: ua,
                visible_seconds,
                active_seconds,
                max_scroll_pct,
            },
        )
        .collect())
}
