//! MCP server (Streamable HTTP at `/mcp`). One tool per verb; results are JSON text blocks.

use axum::{Router, middleware};
use rmcp::{
    RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ErrorData, Implementation, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    api::decode_content,
    app::AppState,
    auth,
    config::host_of,
    db,
    error::{ApiResult, AppError},
    models::{Access, Owner},
    service::{self, FileMeta, Patch, ShareTarget, Upload},
};

pub fn router(state: AppState) -> Router<AppState> {
    let cfg = state.cfg.clone();
    let public_host = host_of(&cfg.public_url);
    let mut hosts = vec![
        public_host.clone(),
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Some((h, _)) = public_host.rsplit_once(':') {
        hosts.push(h.to_string());
    }
    let st = state.clone();
    let service = StreamableHttpService::new(
        move || Ok(HandoutMcp::new(st.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_allowed_hosts(hosts),
    );
    Router::new()
        .nest_service("/mcp", service)
        .route_layer(middleware::from_fn_with_state(state, auth::require_bearer))
}

#[derive(Clone)]
pub struct HandoutMcp {
    st: AppState,
    tool_router: ToolRouter<Self>,
}

fn owner_of(ctx: &RequestContext<RoleServer>) -> Result<Owner, ErrorData> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|p| p.extensions.get::<Owner>())
        .cloned()
        .ok_or_else(|| ErrorData::invalid_request("unauthorized: missing owner", None))
}

fn ok<T: Serialize>(v: &T) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(v)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn result<T: Serialize>(r: ApiResult<T>) -> Result<CallToolResult, ErrorData> {
    match r {
        Ok(v) => ok(&v),
        Err(AppError::Internal(e)) => Err(ErrorData::internal_error(e.to_string(), None)),
        Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(
            e.to_json().to_string(),
        )])),
    }
}

fn opt(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn opt_patch(value: Option<String>, clear: Option<bool>) -> Option<Patch<String>> {
    match (clear.unwrap_or(false), value) {
        (true, _) => Some(Patch::Clear),
        (false, Some(v)) => Some(Patch::Set(v)),
        (false, None) => None,
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UploadFileParams {
    /// Title shown in the recipient's sidebar. Defaults to the filename stem.
    pub title: Option<String>,
    /// Filename including extension (decides the kind: .html .md .pdf .png …). Default `index.html`.
    pub filename: Option<String>,
    /// Text content (HTML, Markdown, …). Use `content_base64` for binary files.
    pub content: Option<String>,
    /// Base64-encoded binary content (PDF, images).
    pub content_base64: Option<String>,
    /// MIME type override, e.g. `application/pdf`.
    pub mime: Option<String>,
    /// Readable prefix of the public id; defaults to a slug of the title.
    pub slug: Option<String>,
    /// `link` (anyone with the link, default), `recipients` (invited only) or `off`.
    pub access: Option<Access>,
    /// Optional password gate.
    pub password: Option<String>,
    /// Email gate: viewers must enter an email, which becomes a recipient.
    pub email_gate: Option<bool>,
    /// RFC 3339 expiry, e.g. `2026-12-31T23:59:59Z`.
    pub expires_at: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateFileParams {
    /// Public file id, e.g. `proposal-7xKq2mNp4R`.
    pub file_id: String,
    pub filename: Option<String>,
    pub content: Option<String>,
    pub content_base64: Option<String>,
    pub mime: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetFileParams {
    pub file_id: String,
    pub title: Option<String>,
    pub access: Option<Access>,
    /// New password. Use `clear_password` to remove the gate.
    pub password: Option<String>,
    pub clear_password: Option<bool>,
    pub email_gate: Option<bool>,
    /// RFC 3339. Use `clear_expiry` to remove.
    pub expires_at: Option<String>,
    pub clear_expiry: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListFilesParams {
    /// Substring match on title or id.
    pub query: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FileIdParams {
    pub file_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FindRecipientsParams {
    /// Substring match on name, email or company. Empty lists everyone.
    pub query: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CreateRecipientParams {
    pub name: String,
    pub email: Option<String>,
    pub company: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateRecipientParams {
    pub recipient_id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub company: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RecipientIdParams {
    pub recipient_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ShareFileParams {
    pub file_id: String,
    /// Existing recipient id. Otherwise the recipient is looked up by email, then by exact name, or created.
    pub recipient_id: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub company: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RevokeInviteParams {
    pub file_id: String,
    pub recipient_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListOpensParams {
    pub file_id: Option<String>,
    pub recipient_id: Option<String>,
    /// RFC 3339 lower bound.
    pub since: Option<String>,
    pub limit: Option<i64>,
}

async fn upload_from(
    st: &AppState,
    content: Option<String>,
    content_base64: Option<String>,
    mime: Option<String>,
    filename: Option<String>,
) -> ApiResult<Upload> {
    let bytes = decode_content(content, content_base64)?;
    if bytes.len() > st.cfg.max_upload_bytes {
        return Err(AppError::TooLarge);
    }
    let blob = st.blobs.put_bytes(&bytes).await?;
    Ok(Upload {
        blob,
        mime: opt(mime),
        filename: opt(filename),
    })
}

#[tool_router]
impl HandoutMcp {
    pub fn new(st: AppState) -> Self {
        Self {
            st,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Who am I: the owner behind this token, public URL and MCP URL.",
        annotations(read_only_hint = true)
    )]
    async fn whoami(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        ok(&service::owner_json(&self.st, &owner))
    }

    #[tool(
        description = "Upload a new file (HTML, Markdown, PDF, image). Returns the file and its file link. Share it with a recipient via share_file."
    )]
    async fn upload_file(
        &self,
        Parameters(p): Parameters<UploadFileParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        let meta = FileMeta {
            title: opt(p.title),
            slug: opt(p.slug),
            access: p.access,
            password: opt(p.password),
            email_gate: p.email_gate,
            expires_at: opt(p.expires_at),
        };
        let r = async {
            let up = upload_from(&self.st, p.content, p.content_base64, p.mime, p.filename).await?;
            service::create_file(&self.st, &owner, meta, up).await
        }
        .await;
        result(r)
    }

    #[tool(
        description = "Upload a new version of an existing file (same links keep working; recipients see it as new)."
    )]
    async fn update_file(
        &self,
        Parameters(p): Parameters<UpdateFileParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        let r = async {
            let up = upload_from(&self.st, p.content, p.content_base64, p.mime, p.filename).await?;
            service::add_version(&self.st, &owner, &p.file_id, up).await
        }
        .await;
        result(r)
    }

    #[tool(
        description = "Change a file's title, access (link|recipients|off), password gate, email gate or expiry."
    )]
    async fn set_file(
        &self,
        Parameters(p): Parameters<SetFileParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        let r = async {
            let patch = service::patch_from_strings(
                opt(p.title),
                p.access,
                opt_patch(opt(p.password), p.clear_password),
                p.email_gate,
                opt_patch(opt(p.expires_at), p.clear_expiry),
            )?;
            service::patch_file(&self.st, &owner, &p.file_id, patch).await
        }
        .await;
        result(r)
    }

    #[tool(
        description = "List files (newest first). Optional substring query.",
        annotations(read_only_hint = true)
    )]
    async fn list_files(
        &self,
        Parameters(p): Parameters<ListFilesParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(
            service::list_files(&self.st, &owner, p.query.as_deref(), p.limit.unwrap_or(50))
                .await
                .map(|files| json!({ "files": files })),
        )
    }

    #[tool(
        description = "Get one file with versions, invites (incl. recipient links) and open count.",
        annotations(read_only_hint = true)
    )]
    async fn get_file(
        &self,
        Parameters(p): Parameters<FileIdParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(service::get_file(&self.st, &owner, &p.file_id).await)
    }

    #[tool(
        description = "Delete a file. Its links stop working immediately.",
        annotations(destructive_hint = true)
    )]
    async fn delete_file(
        &self,
        Parameters(p): Parameters<FileIdParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(
            service::delete_file(&self.st, &owner, &p.file_id)
                .await
                .map(|_| json!({ "deleted": p.file_id })),
        )
    }

    #[tool(
        description = "Find recipients by name, email or company (substring). Empty query lists all.",
        annotations(read_only_hint = true)
    )]
    async fn find_recipients(
        &self,
        Parameters(p): Parameters<FindRecipientsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(
            service::find_recipients(&self.st, &owner, p.query.as_deref(), p.limit.unwrap_or(50))
                .await
                .map(|rs| json!({ "recipients": rs.iter().map(service::recipient_json).collect::<Vec<_>>() })),
        )
    }

    #[tool(
        description = "Create a recipient identity (name, optional email/company). Usually share_file does this for you."
    )]
    async fn create_recipient(
        &self,
        Parameters(p): Parameters<CreateRecipientParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(
            service::create_recipient(
                &self.st,
                &owner,
                &p.name,
                p.email.as_deref(),
                p.company.as_deref(),
            )
            .await
            .map(|r| service::recipient_json(&r)),
        )
    }

    #[tool(description = "Update a recipient's name, email or company.")]
    async fn update_recipient(
        &self,
        Parameters(p): Parameters<UpdateRecipientParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        let email = opt(p.email).map(Patch::Set);
        let company = opt(p.company).map(Patch::Set);
        result(
            service::update_recipient(
                &self.st,
                &owner,
                &p.recipient_id,
                p.name.as_deref(),
                email,
                company,
            )
            .await
            .map(|r| service::recipient_json(&r)),
        )
    }

    #[tool(
        description = "Get a recipient with every file shared with them and their recipient links.",
        annotations(read_only_hint = true)
    )]
    async fn get_recipient(
        &self,
        Parameters(p): Parameters<RecipientIdParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(service::recipient_detail(&self.st, &owner, &p.recipient_id).await)
    }

    #[tool(
        description = "Rotate a recipient's token: all their old recipient links stop auto-logging in."
    )]
    async fn rotate_recipient_token(
        &self,
        Parameters(p): Parameters<RecipientIdParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(
            service::rotate_recipient_token(&self.st, &owner, &p.recipient_id)
                .await
                .map(|r| service::recipient_json(&r)),
        )
    }

    #[tool(
        description = "Share a file with a recipient: finds (by recipient_id, email, or exact name) or creates the recipient, records the invite and returns the recipient link to send them. The file shows up in their sidebar."
    )]
    async fn share_file(
        &self,
        Parameters(p): Parameters<ShareFileParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        let target = match opt(p.recipient_id) {
            Some(id) => ShareTarget::Id(id),
            None => ShareTarget::Lookup {
                name: opt(p.name),
                email: opt(p.email),
                company: opt(p.company),
            },
        };
        result(service::share_file(&self.st, &owner, &p.file_id, target).await)
    }

    #[tool(
        description = "Remove a recipient's invite to a file (it disappears from their sidebar; link-access files stay reachable by link)."
    )]
    async fn revoke_invite(
        &self,
        Parameters(p): Parameters<RevokeInviteParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        result(
            service::revoke_invite(&self.st, &owner, &p.file_id, &p.recipient_id)
                .await
                .map(|_| json!({ "revoked": true })),
        )
    }

    #[tool(
        description = "List opens (views): when, which file/version, which recipient (or anonymous).",
        annotations(read_only_hint = true)
    )]
    async fn list_opens(
        &self,
        Parameters(p): Parameters<ListOpensParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = owner_of(&ctx)?;
        let r = async {
            let since = match p.since.as_deref() {
                Some(s) => Some(
                    db::parse_rfc3339(s).ok_or_else(|| AppError::bad("since must be RFC 3339"))?,
                ),
                None => None,
            };
            service::list_opens(
                &self.st,
                &owner,
                p.file_id.as_deref(),
                p.recipient_id.as_deref(),
                since,
                p.limit.unwrap_or(100),
            )
            .await
            .map(|opens| json!({ "opens": opens }))
        }
        .await;
        result(r)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for HandoutMcp {
    fn get_info(&self) -> ServerConfig {
        let mut server_info = Implementation::from_build_env();
        server_info.name = "handout".into();
        server_info.title = Some("handout".into());
        server_info.version = env!("CARGO_PKG_VERSION").into();
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = server_info;
        info.instructions = Some(
            "handout shares files (HTML, Markdown, PDF, images) with recipients via links. \
             Typical flow: upload_file → share_file(file_id, name/email) → send the returned recipient_link. \
             A recipient keeps one identity: every file shared with them appears in their sidebar. \
             Use update_file to publish a new version under the same links, list_opens to see who opened what."
                .into(),
        );
        info
    }
}
