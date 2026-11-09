use std::{
    convert::Infallible,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use anyhow::Result;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderValue, header},
    response::Response,
};
use tower::Service;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};

use crate::{
    api, auth,
    blobs::BlobStore,
    config::{Config, host_of},
    content,
    db::{self, Db},
    mcp, oauth,
    ratelimit::RateLimiter,
    shell, view,
};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub blobs: Arc<BlobStore>,
    pub cfg: Arc<Config>,
    pub limiter: Arc<RateLimiter>,
}

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let db = db::connect(&cfg.db_path()).await?;
    let blobs = Arc::new(BlobStore::new(cfg.blob_dir())?);
    Ok(AppState {
        db,
        blobs,
        cfg: Arc::new(cfg),
        limiter: Arc::new(RateLimiter::default()),
    })
}

/// Dispatches by `Host`: the content origin gets the raw-file router, everything else the shell.
#[derive(Clone)]
pub struct HostRouter {
    shell: Router,
    content: Router,
    content_host: String,
}

impl Service<Request> for HostRouter {
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .map(|h| h.trim().to_ascii_lowercase())
            .unwrap_or_default();
        let mut router = if host == self.content_host {
            self.content.clone()
        } else {
            self.shell.clone()
        };
        Box::pin(async move { router.call(req).await })
    }
}

/// Shell CSP. `form_action` is appended to `'self'`; browsers also enforce it on the redirect after a
/// form POST, so the OAuth consent page must list the client's redirect origin.
pub fn shell_csp(content_url: &str, form_action: &str) -> String {
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
         font-src 'self'; connect-src 'self'; frame-src {content_url}; base-uri 'self'; form-action 'self'{form_action}; frame-ancestors 'none'"
    )
}

pub fn build_app(state: AppState) -> HostRouter {
    let cfg = state.cfg.clone();
    let shell_csp = shell_csp(&cfg.content_url, "");
    let shell = Router::new()
        .merge(shell::router())
        .merge(view::router())
        .merge(oauth::router())
        .merge(crate::webauth::router())
        .nest("/api", api::router())
        .merge(mcp::router(state.clone()))
        .layer(DefaultBodyLimit::max(cfg.max_upload_bytes + 1024 * 1024))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&shell_csp).unwrap(),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());
    let content = content::router()
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    HostRouter {
        shell,
        content,
        content_host: host_of(&cfg.content_url).to_ascii_lowercase(),
    }
}

pub async fn serve(
    state: AppState,
    listener: tokio::net::TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    use axum::ServiceExt;
    let app = build_app(state);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await?;
    Ok(())
}

pub use auth::OwnerAuth;
