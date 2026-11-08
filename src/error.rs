use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized { resource_metadata: Option<String> },
    #[error("forbidden")]
    Forbidden,
    #[error("{message}")]
    Conflict { message: String, data: Value },
    #[error("too many requests")]
    RateLimited,
    #[error("payload too large")]
    TooLarge,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn conflict(msg: impl Into<String>, data: Value) -> Self {
        Self::Conflict {
            message: msg.into(),
            data,
        }
    }
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized { .. } => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Conflict { .. } => "conflict",
            Self::RateLimited => "rate_limited",
            Self::TooLarge => "too_large",
            Self::Internal(_) => "internal",
        }
    }
    pub fn to_json(&self) -> Value {
        let mut body = json!({ "error": { "code": self.code(), "message": self.to_string() } });
        if let Self::Conflict { data, .. } = self {
            body["error"]["data"] = data.clone();
        }
        body
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::Internal(e) = &self {
            tracing::error!(error = ?e, "internal error");
        }
        let mut resp = (self.status(), Json(self.to_json())).into_response();
        if let Self::Unauthorized {
            resource_metadata: Some(url),
        } = &self
        {
            resp.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_str(&format!("Bearer resource_metadata=\"{url}\"")).unwrap(),
            );
        }
        resp
    }
}

pub type ApiResult<T> = Result<T, AppError>;
