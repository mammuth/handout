//! Viewer side: `/s/{owner}/{file}` entry, recipient auto-login, viewer API (cookies), gates, opens.

use std::{net::SocketAddr, time::Duration};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    app::AppState,
    auth,
    db::{self, rfc3339},
    error::{ApiResult, AppError},
    ids,
    models::{Access, FileRow, FileVersion, Kind, Owner, Recipient},
    service, shell,
};

pub const RECIPIENT_COOKIE: &str = "hr";
const UNLOCK_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const RECIPIENT_TTL: Duration = Duration::from_secs(365 * 24 * 3600);
pub const CONTENT_SIG_TTL: i64 = 3600;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/s/{owner}/{file}", get(viewer_entry))
        .route("/s/{owner}", get(home_entry))
        .route("/api/view/{owner}", get(view_home))
        .route("/api/view/{owner}/{file}", get(view_file))
        .route("/api/view/{owner}/{file}/unlock", post(unlock))
        .route("/api/view/{owner}/{file}/identify", post(identify))
        .route("/api/view/{owner}/{file}/heartbeat", post(heartbeat))
}

#[derive(Deserialize)]
struct EntryQuery {
    r: Option<String>,
}

/// `?r={token}` → set recipient cookie, redirect to the clean URL. Otherwise serve the SPA.
async fn viewer_entry(
    State(st): State<AppState>,
    Path((owner_slug, file_id)): Path<(String, String)>,
    Query(q): Query<EntryQuery>,
    jar: CookieJar,
) -> Response {
    match q.r.as_deref().filter(|t| !t.is_empty()) {
        Some(token) => {
            login_and_redirect(
                &st,
                &owner_slug,
                token,
                &format!("/s/{owner_slug}/{file_id}"),
                jar,
            )
            .await
        }
        None => shell::spa_index().into_response(),
    }
}

async fn home_entry(
    State(st): State<AppState>,
    Path(owner_slug): Path<String>,
    Query(q): Query<EntryQuery>,
    jar: CookieJar,
) -> Response {
    match q.r.as_deref().filter(|t| !t.is_empty()) {
        Some(token) => {
            login_and_redirect(&st, &owner_slug, token, &format!("/s/{owner_slug}"), jar).await
        }
        None => shell::spa_index().into_response(),
    }
}

async fn login_and_redirect(
    st: &AppState,
    owner_slug: &str,
    token: &str,
    clean: &str,
    jar: CookieJar,
) -> Response {
    let recipient = match service::owner_by_slug(&st.db, owner_slug).await {
        Ok(Some(owner)) => service::recipient_by_token(st, owner.id, token)
            .await
            .ok()
            .flatten(),
        _ => None,
    };
    let jar = match recipient {
        Some(r) => {
            let _ = sqlx::query("UPDATE recipients SET last_seen_at = ? WHERE id = ?")
                .bind(db::now())
                .bind(&r.id)
                .execute(&st.db)
                .await;
            jar.add(auth::cookie(
                &st.cfg,
                RECIPIENT_COOKIE,
                r.token.clone(),
                Some(RECIPIENT_TTL),
            ))
        }
        None => jar,
    };
    (jar, Redirect::to(clean)).into_response()
}

/// The file's owner, signed in (web session) in this browser: skips access/password/email gates; opens unrecorded.
async fn is_owner_session(st: &AppState, owner: &Owner, jar: &CookieJar) -> bool {
    let Some(c) = jar.get(&st.cfg.cookie_name(crate::oauth::SESSION_COOKIE)) else {
        return false;
    };
    matches!(auth::session_owner(&st.db, c.value()).await, Ok(Some(o)) if o.id == owner.id)
}

async fn current_recipient(st: &AppState, owner_id: i64, jar: &CookieJar) -> Option<Recipient> {
    let token = jar
        .get(&st.cfg.cookie_name(RECIPIENT_COOKIE))?
        .value()
        .to_string();
    service::recipient_by_token(st, owner_id, &token)
        .await
        .ok()
        .flatten()
}

#[derive(Serialize)]
pub struct SidebarEntry {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    pub updated_at: String,
    pub is_new: bool,
    pub link: String,
}

async fn sidebar(
    st: &AppState,
    owner: &Owner,
    recipient: &Recipient,
) -> ApiResult<Vec<SidebarEntry>> {
    let rows = sqlx::query_as::<_, (String, String, String, Option<i64>, i64, i64)>(
        "SELECT f.public_id, f.title, f.kind, i.last_opened_version, \
                (SELECT MAX(v.created_at) FROM file_versions v WHERE v.file_id = f.id) AS latest_at, \
                (SELECT MAX(v.number) FROM file_versions v WHERE v.file_id = f.id) AS latest_no \
         FROM invites i JOIN files f ON f.id = i.file_id \
         WHERE i.recipient_id = ? AND f.deleted_at IS NULL AND f.access != 'off' \
           AND (f.expires_at IS NULL OR f.expires_at > ?) \
         ORDER BY latest_at DESC",
    )
    .bind(&recipient.id)
    .bind(db::now())
    .fetch_all(&st.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, title, kind, last_opened_version, latest_at, latest_no)| SidebarEntry {
                link: format!("/s/{}/{}", owner.slug, id),
                id,
                title,
                kind: Kind::parse(&kind),
                updated_at: rfc3339(latest_at),
                is_new: last_opened_version.map(|v| v < latest_no).unwrap_or(true),
            },
        )
        .collect())
}

fn unlock_cookie_name(public_id: &str) -> String {
    format!("hu_{public_id}")
}

fn unlock_value(st: &AppState, file: &FileRow) -> String {
    ids::hmac_b64(
        &st.cfg.secret,
        format!(
            "unlock|{}|{}",
            file.public_id,
            file.password_hash.as_deref().unwrap_or("")
        )
        .as_bytes(),
    )
}

fn is_unlocked(st: &AppState, file: &FileRow, jar: &CookieJar) -> bool {
    jar.get(&st.cfg.cookie_name(&unlock_cookie_name(&file.public_id)))
        .map(|c| {
            subtle::ConstantTimeEq::ct_eq(c.value().as_bytes(), unlock_value(st, file).as_bytes())
                .into()
        })
        .unwrap_or(false)
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ViewState {
    Ok,
    Password,
    Email,
    Expired,
    Forbidden,
    Off,
}

fn evaluate(
    file: &FileRow,
    is_owner: bool,
    recipient: Option<&Recipient>,
    invited: bool,
    unlocked: bool,
) -> ViewState {
    if file.access() == Access::Off {
        return ViewState::Off;
    }
    if file.expires_at.is_some_and(|e| e <= db::now()) {
        return ViewState::Expired;
    }
    if is_owner {
        return ViewState::Ok;
    }
    if file.access() == Access::Recipients && !(recipient.is_some() && invited) {
        return ViewState::Forbidden;
    }
    if file.password_hash.is_some() && !unlocked {
        return ViewState::Password;
    }
    if file.email_gate && recipient.is_none() {
        return ViewState::Email;
    }
    ViewState::Ok
}

pub fn sign_content(
    st: &AppState,
    owner_slug: &str,
    public_id: &str,
    version: i64,
    exp: i64,
) -> String {
    let sig = ids::hmac_b64(
        &st.cfg.secret,
        format!("content|{owner_slug}|{public_id}|{version}|{exp}").as_bytes(),
    );
    format!("{exp}.{sig}")
}

pub fn verify_content_sig(
    st: &AppState,
    owner_slug: &str,
    public_id: &str,
    version: i64,
    t: &str,
) -> bool {
    let Some((exp, sig)) = t.split_once('.') else {
        return false;
    };
    let Ok(exp) = exp.parse::<i64>() else {
        return false;
    };
    if exp < db::now() {
        return false;
    }
    ids::hmac_verify(
        &st.cfg.secret,
        format!("content|{owner_slug}|{public_id}|{version}|{exp}").as_bytes(),
        sig,
    )
}

fn content_urls(st: &AppState, owner: &Owner, file: &FileRow, v: &FileVersion) -> (String, String) {
    let exp = db::now() + CONTENT_SIG_TTL;
    let t = sign_content(st, &owner.slug, &file.public_id, v.number, exp);
    let base = format!("{}/{}/{}", st.cfg.content_url, owner.slug, file.public_id);
    (
        format!("{base}?v={}&t={t}", v.number),
        format!("{base}/download?v={}&t={t}", v.number),
    )
}

pub fn client_ip(headers: &HeaderMap, addr: Option<SocketAddr>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "unknown".into())
}

fn ip_hash(st: &AppState, ip: &str) -> String {
    let day = db::now() / 86_400;
    ids::sha256_hex(format!("{ip}|{day}|{}", ids::b64url(&st.cfg.secret)).as_bytes())[..32]
        .to_string()
}

fn user_agent(headers: &HeaderMap) -> String {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect())
        .unwrap_or_default()
}

async fn load_owner_file(
    st: &AppState,
    owner_slug: &str,
    file_id: &str,
) -> ApiResult<(Owner, FileRow)> {
    let owner = service::owner_by_slug(&st.db, owner_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let file = service::load_file(st, owner.id, file_id).await?;
    Ok((owner, file))
}

async fn is_invited(st: &AppState, file_id: i64, recipient_id: &str) -> ApiResult<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM invites WHERE file_id = ? AND recipient_id = ?")
            .bind(file_id)
            .bind(recipient_id)
            .fetch_one(&st.db)
            .await?;
    Ok(n > 0)
}

async fn view_file(
    State(st): State<AppState>,
    Path((owner_slug, file_id)): Path<(String, String)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
) -> ApiResult<Json<Value>> {
    let (owner, file) = load_owner_file(&st, &owner_slug, &file_id).await?;
    let latest = service::latest_version(&st, file.id).await?;
    let recipient = current_recipient(&st, owner.id, &jar).await;
    let invited = match &recipient {
        Some(r) => is_invited(&st, file.id, &r.id).await?,
        None => false,
    };
    let is_owner = is_owner_session(&st, &owner, &jar).await;
    let state = evaluate(
        &file,
        is_owner,
        recipient.as_ref(),
        invited,
        is_unlocked(&st, &file, &jar),
    );

    let mut content_url = None;
    let mut download_url = None;
    let mut open = None;
    if state == ViewState::Ok {
        let (c, d) = content_urls(&st, &owner, &file, &latest);
        content_url = Some(c);
        download_url = Some(d);
    }
    if state == ViewState::Ok && !is_owner {
        let now = db::now();
        let open_id = sqlx::query("INSERT INTO opens (file_id, version_number, recipient_id, ip_hash, user_agent, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(file.id)
            .bind(latest.number)
            .bind(recipient.as_ref().map(|r| r.id.clone()))
            .bind(ip_hash(&st, &client_ip(&headers, Some(addr))))
            .bind(user_agent(&headers))
            .bind(now)
            .execute(&st.db)
            .await?
            .last_insert_rowid();
        open = Some(json!({ "id": open_id, "token": open_token(&st, open_id) }));
        if let Some(r) = &recipient {
            if invited {
                sqlx::query("UPDATE invites SET last_opened_at = ?, last_opened_version = ? WHERE file_id = ? AND recipient_id = ?")
                    .bind(now)
                    .bind(latest.number)
                    .bind(file.id)
                    .bind(&r.id)
                    .execute(&st.db)
                    .await?;
            }
            sqlx::query("UPDATE recipients SET last_seen_at = ? WHERE id = ?")
                .bind(now)
                .bind(&r.id)
                .execute(&st.db)
                .await?;
        }
    }
    let sidebar = match &recipient {
        Some(r) => sidebar(&st, &owner, r).await?,
        None => vec![],
    };
    Ok(Json(json!({
        "owner": { "slug": owner.slug, "name": owner.name },
        "recipient": recipient.as_ref().map(|r| json!({ "name": r.name, "email": r.email })),
        "file": {
            "id": file.public_id, "title": file.title, "kind": file.kind(), "version": latest.number,
            "filename": latest.filename, "size": latest.size, "mime": latest.mime,
            "updated_at": rfc3339(latest.created_at),
        },
        "state": state,
        "content_url": content_url,
        "download_url": download_url,
        "open": open,
        "sidebar": sidebar,
    })))
}

fn open_token(st: &AppState, open_id: i64) -> String {
    ids::hmac_b64(&st.cfg.secret, format!("open|{open_id}").as_bytes())
}

#[derive(Deserialize)]
struct HeartbeatBody {
    open_id: i64,
    open_token: String,
    #[serde(default)]
    visible_seconds: i64,
    #[serde(default)]
    active_seconds: i64,
    #[serde(default)]
    max_scroll_pct: i64,
}

/// Engagement report from the shell for one open. Body is JSON, sent via fetch or `sendBeacon`
/// (hence parsed from raw bytes: sendBeacon cannot set a JSON content type).
async fn heartbeat(
    State(st): State<AppState>,
    Path((owner_slug, file_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<StatusCode> {
    check_origin(&st, &headers)?;
    let hb: HeartbeatBody =
        serde_json::from_slice(&body).map_err(|_| AppError::bad("invalid heartbeat"))?;
    if !ids::hmac_verify(
        &st.cfg.secret,
        format!("open|{}", hb.open_id).as_bytes(),
        &hb.open_token,
    ) {
        return Err(AppError::Forbidden);
    }
    let (_owner, file) = load_owner_file(&st, &owner_slug, &file_id).await?;
    let now = db::now();
    // one heartbeat covers at most a minute; opens older than a day stop accepting reports
    let visible = hb.visible_seconds.clamp(0, 60);
    let active = hb.active_seconds.clamp(0, visible);
    let scroll = hb.max_scroll_pct.clamp(0, 100);
    let n = sqlx::query(
        "UPDATE opens SET visible_seconds = visible_seconds + ?, active_seconds = active_seconds + ?, \
         max_scroll_pct = MAX(max_scroll_pct, ?), last_heartbeat_at = ? \
         WHERE id = ? AND file_id = ? AND created_at > ?",
    )
    .bind(visible)
    .bind(active)
    .bind(scroll)
    .bind(now)
    .bind(hb.open_id)
    .bind(file.id)
    .bind(now - 86_400)
    .execute(&st.db)
    .await?
    .rows_affected();
    if n == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

async fn view_home(
    State(st): State<AppState>,
    Path(owner_slug): Path<String>,
    jar: CookieJar,
) -> ApiResult<Json<Value>> {
    let owner = service::owner_by_slug(&st.db, &owner_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let recipient = current_recipient(&st, owner.id, &jar)
        .await
        .ok_or(AppError::Forbidden)?;
    let files = sidebar(&st, &owner, &recipient).await?;
    Ok(Json(json!({
        "owner": { "slug": owner.slug, "name": owner.name },
        "recipient": { "name": recipient.name, "email": recipient.email },
        "sidebar": files,
    })))
}

pub fn check_origin(st: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if origin.trim_end_matches('/') == st.cfg.public_url {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[derive(Deserialize)]
struct UnlockBody {
    password: String,
}

async fn unlock(
    State(st): State<AppState>,
    Path((owner_slug, file_id)): Path<(String, String)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<UnlockBody>,
) -> ApiResult<Response> {
    check_origin(&st, &headers)?;
    let ip = client_ip(&headers, Some(addr));
    if !st
        .limiter
        .check(&format!("unlock:{ip}"), 10, Duration::from_secs(300))
    {
        return Err(AppError::RateLimited);
    }
    let (_owner, file) = load_owner_file(&st, &owner_slug, &file_id).await?;
    let Some(hash) = &file.password_hash else {
        return Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response());
    };
    if !auth::verify_password(&body.password, hash) {
        return Err(AppError::Forbidden);
    }
    let jar = jar.add(auth::cookie(
        &st.cfg,
        &unlock_cookie_name(&file.public_id),
        unlock_value(&st, &file),
        Some(UNLOCK_TTL),
    ));
    Ok((jar, Json(json!({ "ok": true }))).into_response())
}

#[derive(Deserialize)]
struct IdentifyBody {
    email: String,
    name: Option<String>,
}

async fn identify(
    State(st): State<AppState>,
    Path((owner_slug, file_id)): Path<(String, String)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<IdentifyBody>,
) -> ApiResult<Response> {
    check_origin(&st, &headers)?;
    let ip = client_ip(&headers, Some(addr));
    if !st
        .limiter
        .check(&format!("identify:{ip}"), 20, Duration::from_secs(300))
    {
        return Err(AppError::RateLimited);
    }
    let (owner, file) = load_owner_file(&st, &owner_slug, &file_id).await?;
    if !file.email_gate {
        return Err(AppError::bad("this file has no email gate"));
    }
    let email = body.email.trim().to_ascii_lowercase();
    if !email.contains('@') {
        return Err(AppError::bad("invalid email"));
    }
    let recipient = match service::recipient_by_email(&st, owner.id, &email).await? {
        Some(r) => r,
        None => {
            service::create_recipient(
                &st,
                &owner,
                body.name.as_deref().unwrap_or(""),
                Some(&email),
                None,
            )
            .await?
        }
    };
    sqlx::query(
        "INSERT OR IGNORE INTO invites (file_id, recipient_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(file.id)
    .bind(&recipient.id)
    .bind(db::now())
    .execute(&st.db)
    .await?;
    let jar = jar.add(auth::cookie(
        &st.cfg,
        RECIPIENT_COOKIE,
        recipient.token.clone(),
        Some(RECIPIENT_TTL),
    ));
    Ok((jar, Json(json!({ "ok": true }))).into_response())
}
