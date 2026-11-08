use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct Owner {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub password_hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Html,
    Markdown,
    Pdf,
    Image,
    Other,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Html => "html",
            Kind::Markdown => "markdown",
            Kind::Pdf => "pdf",
            Kind::Image => "image",
            Kind::Other => "other",
        }
    }
    pub fn parse(s: &str) -> Kind {
        match s {
            "html" => Kind::Html,
            "markdown" => Kind::Markdown,
            "pdf" => Kind::Pdf,
            "image" => Kind::Image,
            _ => Kind::Other,
        }
    }
    /// Decide the kind from the declared mime first, then the filename extension.
    pub fn detect(mime: &str, filename: &str) -> Kind {
        let mime = mime
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match mime.as_str() {
            "text/html" | "application/xhtml+xml" => return Kind::Html,
            "text/markdown" | "text/x-markdown" => return Kind::Markdown,
            "application/pdf" => return Kind::Pdf,
            m if m.starts_with("image/") => return Kind::Image,
            _ => {}
        }
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "html" | "htm" => Kind::Html,
            "md" | "markdown" => Kind::Markdown,
            "pdf" => Kind::Pdf,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" => Kind::Image,
            _ => Kind::Other,
        }
    }
    /// Content type to serve this kind with when the upload did not declare a useful one.
    pub fn default_mime(self, filename: &str) -> String {
        match self {
            Kind::Html => "text/html; charset=utf-8".into(),
            Kind::Markdown => "text/markdown; charset=utf-8".into(),
            Kind::Pdf => "application/pdf".into(),
            _ => mime_guess::from_path(filename)
                .first_raw()
                .unwrap_or("application/octet-stream")
                .to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Anyone with the file link.
    Link,
    /// Only invited recipients (via recipient link / cookie).
    Recipients,
    /// Nobody; link is revoked.
    Off,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Link => "link",
            Access::Recipients => "recipients",
            Access::Off => "off",
        }
    }
    pub fn parse(s: &str) -> Access {
        match s {
            "recipients" => Access::Recipients,
            "off" => Access::Off,
            _ => Access::Link,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct FileRow {
    pub id: i64,
    pub owner_id: i64,
    pub public_id: String,
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub access: String,
    pub password_hash: Option<String>,
    pub email_gate: bool,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

impl FileRow {
    pub fn kind(&self) -> Kind {
        Kind::parse(&self.kind)
    }
    pub fn access(&self) -> Access {
        Access::parse(&self.access)
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct FileVersion {
    pub id: i64,
    pub file_id: i64,
    pub number: i64,
    pub blob_sha256: String,
    pub size: i64,
    pub mime: String,
    pub filename: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct Recipient {
    pub id: String,
    pub owner_id: i64,
    pub name: String,
    pub email: Option<String>,
    pub company: Option<String>,
    pub token: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Invite {
    pub file_id: i64,
    pub recipient_id: String,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ApiKey {
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
    pub key_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}
