//! End-to-end tests against a real listener: REST, viewer flow, content host, gates, OAuth, MCP.

use std::sync::Arc;

use handout::{AppState, Config, app, models::Owner, service};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

struct App {
    /// Shell origin (also PUBLIC_URL).
    base: String,
    state: AppState,
    owner: Owner,
    key: String,
    _tmp: tempfile::TempDir,
}

async fn spawn() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    let content = format!("http://localhost:{port}");
    let cfg = Config::for_tests(tmp.path().to_path_buf(), &base, &content);
    let state = app::build_state(cfg).await.unwrap();
    let owner = service::create_owner(&state.db, "max", "Max", "password123")
        .await
        .unwrap();
    let (_, key) = service::create_api_key(&state, &owner, "test")
        .await
        .unwrap();
    let st = state.clone();
    tokio::spawn(async move {
        app::serve(st, listener, std::future::pending())
            .await
            .unwrap()
    });
    App {
        base,
        state,
        owner,
        key,
        _tmp: tmp,
    }
}

fn browser() -> Client {
    Client::builder().cookie_store(true).build().unwrap()
}

fn no_redirect_browser() -> Client {
    Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

impl App {
    fn api(&self) -> Client {
        let mut h = header::HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", self.key).parse().unwrap(),
        );
        Client::builder().default_headers(h).build().unwrap()
    }
    fn url(&self, p: &str) -> String {
        format!("{}{}", self.base, p)
    }
    async fn upload_json(&self, title: &str, filename: &str, content: &str, extra: Value) -> Value {
        let mut body = json!({ "title": title, "filename": filename, "content": content });
        if let Value::Object(m) = extra {
            for (k, v) in m {
                body[k] = v;
            }
        }
        let res = self
            .api()
            .post(self.url("/api/files"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::CREATED,
            "{}",
            res.text().await.unwrap()
        );
        res.json().await.unwrap()
    }
    async fn view(&self, client: &Client, file_id: &str) -> (StatusCode, Value) {
        let res = client
            .get(self.url(&format!("/api/view/max/{file_id}")))
            .send()
            .await
            .unwrap();
        let status = res.status();
        (status, res.json().await.unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn files_sharing_and_viewing() {
    let app = spawn().await;

    // upload (JSON)
    let f1 = app
        .upload_json(
            "Acme Proposal",
            "proposal.html",
            "<h1>Hello Acme</h1>",
            json!({}),
        )
        .await;
    let f1_id = f1["id"].as_str().unwrap().to_string();
    assert!(f1_id.starts_with("acme-proposal-"), "{f1_id}");
    assert_eq!(f1["kind"], "html");
    assert_eq!(f1["file_link"], format!("{}/s/max/{}", app.base, f1_id));

    // share by name+email → recipient created
    let s1: Value = app
        .api()
        .post(app.url(&format!("/api/files/{f1_id}/share")))
        .json(&json!({ "name": "Jon Doe", "email": "Jon@Acme.com" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(s1["recipient_created"], true);
    assert_eq!(s1["invite_created"], true);
    assert_eq!(s1["recipient"]["email"], "jon@acme.com");
    let jon_id = s1["recipient"]["id"].as_str().unwrap().to_string();
    let link = s1["recipient_link"].as_str().unwrap().to_string();
    assert!(link.starts_with(&format!("{}/s/max/{}?r=", app.base, f1_id)));

    // share again by name only → reused, no new invite
    let s2: Value = app
        .api()
        .post(app.url(&format!("/api/files/{f1_id}/share")))
        .json(&json!({ "name": "jon doe" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(s2["recipient"]["id"], jon_id);
    assert_eq!(s2["invite_created"], false);
    assert_eq!(s2["recipient_created"], false);

    // Jon opens the recipient link: redirect to clean URL, cookie set, SPA served
    let jon = browser();
    let res = jon.get(&link).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.url().query(), None, "token stripped from URL");
    let html = res.text().await.unwrap();
    assert!(html.contains("<div id=\"app\">"), "SPA index served");

    let (st, v) = app.view(&jon, &f1_id).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["state"], "ok");
    assert_eq!(v["recipient"]["name"], "Jon Doe");
    assert_eq!(v["file"]["version"], 1);
    assert_eq!(v["sidebar"].as_array().unwrap().len(), 1);
    assert_eq!(v["sidebar"][0]["is_new"], false, "just opened");
    let content_url = v["content_url"].as_str().unwrap().to_string();
    assert!(content_url.starts_with(&format!(
        "http://localhost:{}/max/{}",
        app.base.rsplit(':').next().unwrap(),
        f1_id
    )));

    // content host serves the raw HTML with sandbox CSP
    let res = reqwest::get(&content_url).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_string();
    assert!(csp.contains("sandbox allow-scripts"), "{csp}");
    assert!(
        csp.contains(&format!("frame-ancestors {}", app.base)),
        "{csp}"
    );
    assert_eq!(
        res.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    let raw = res.text().await.unwrap();
    assert!(
        raw.starts_with("<h1>Hello Acme</h1><script>"),
        "beacon appended to body-less HTML: {raw}"
    );

    // tampered signature → 403
    let bad = content_url.replace("&t=", "&t=1");
    assert_eq!(
        reqwest::get(&bad).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    // content path on the shell host is not served
    let on_shell = content_url.replace(
        &format!("http://localhost:{}", app.base.rsplit(':').next().unwrap()),
        &app.base,
    );
    assert_eq!(
        reqwest::get(&on_shell).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    // anonymous viewer: link access, no sidebar
    let anon = browser();
    let (st, v) = app.view(&anon, &f1_id).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["state"], "ok");
    assert!(v["recipient"].is_null());
    assert_eq!(v["sidebar"].as_array().unwrap().len(), 0);

    // second file shared with Jon by name → appears as new in his sidebar
    let f2 = app
        .upload_json(
            "Pricing",
            "pricing.md",
            "# Pricing\n\n| a | b |\n|---|---|\n| 1 | 2 |",
            json!({}),
        )
        .await;
    let f2_id = f2["id"].as_str().unwrap().to_string();
    assert_eq!(f2["kind"], "markdown");
    let res = app
        .api()
        .post(app.url(&format!("/api/files/{f2_id}/share")))
        .json(&json!({ "name": "Jon Doe" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = app.view(&jon, &f1_id).await;
    let sb = v["sidebar"].as_array().unwrap();
    assert_eq!(sb.len(), 2);
    let pricing = sb.iter().find(|e| e["id"] == f2_id).unwrap();
    assert_eq!(pricing["is_new"], true);
    assert_eq!(pricing["link"], format!("/s/max/{f2_id}"));

    // markdown is rendered to HTML on the content host
    let (_, v) = app.view(&jon, &f2_id).await;
    let md_html = reqwest::get(v["content_url"].as_str().unwrap())
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        md_html.contains("<h1>Pricing</h1>") && md_html.contains("<table>"),
        "{md_html}"
    );

    // new version via multipart PUT
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::text("<h1>Hello Acme v2</h1>")
            .file_name("proposal.html")
            .mime_str("text/html")
            .unwrap(),
    );
    let res = app
        .api()
        .put(app.url(&format!("/api/files/{f1_id}/content")))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v2: Value = res.json().await.unwrap();
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["versions"].as_array().unwrap().len(), 2);
    let (_, v) = app.view(&anon, &f1_id).await;
    assert_eq!(v["file"]["version"], 2);
    let (_, vj) = app.view(&jon, &f2_id).await;
    assert_eq!(
        vj["sidebar"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == f1_id)
            .unwrap()["is_new"],
        true,
        "new version marks file new again"
    );

    // opens attributed
    let opens: Value = app
        .api()
        .get(app.url("/api/opens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let opens = opens["opens"].as_array().unwrap();
    assert!(opens.len() >= 4);
    assert!(
        opens
            .iter()
            .any(|o| o["recipient"]["name"] == "Jon Doe" && o["file_id"] == f1_id)
    );
    assert!(opens.iter().any(|o| o["recipient"].is_null()));

    // file detail lists invites with recipient links; recipient detail lists files
    let d: Value = app
        .api()
        .get(app.url(&format!("/api/files/{f1_id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(d["invites"][0]["recipient"]["id"], jon_id);
    assert!(
        d["invites"][0]["recipient_link"]
            .as_str()
            .unwrap()
            .contains("?r=")
    );
    assert!(d["opens"].as_i64().unwrap() >= 2);
    let r: Value = app
        .api()
        .get(app.url(&format!("/api/recipients/{jon_id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 2);
    let found: Value = app
        .api()
        .get(app.url("/api/recipients?q=acme"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(found["recipients"][0]["id"], jon_id);

    // rotate token → old recipient link no longer logs in
    let rotated: Value = app
        .api()
        .post(app.url(&format!("/api/recipients/{jon_id}/rotate-token")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(rotated["token"], link.rsplit("?r=").next().unwrap());
    let fresh = browser();
    fresh.get(&link).send().await.unwrap();
    let (_, v) = app.view(&fresh, &f1_id).await;
    assert!(v["recipient"].is_null());
    // Jon's old cookie is dead too; the new recipient link logs him back in
    let (_, v) = app.view(&jon, &f1_id).await;
    assert!(v["recipient"].is_null());
    jon.get(format!(
        "{}/s/max/{}?r={}",
        app.base,
        f1_id,
        rotated["token"].as_str().unwrap()
    ))
    .send()
    .await
    .unwrap();
    let (_, v) = app.view(&jon, &f1_id).await;
    assert_eq!(v["recipient"]["name"], "Jon Doe");

    // revoke invite → gone from sidebar; delete → 404
    let res = app
        .api()
        .delete(app.url(&format!("/api/files/{f2_id}/invites/{jon_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (_, v) = app.view(&jon, &f1_id).await;
    assert_eq!(v["sidebar"].as_array().unwrap().len(), 1);
    let res = app
        .api()
        .delete(app.url(&format!("/api/files/{f2_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (st, _) = app.view(&jon, &f2_id).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // unauthenticated API → 401 with resource metadata hint
    let res = reqwest::get(app.url("/api/files")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(
        res.headers()[header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap()
            .contains("oauth-protected-resource")
    );
}

#[tokio::test]
async fn gates_and_access() {
    let app = spawn().await;
    let f = app
        .upload_json(
            "Secret",
            "secret.html",
            "<p>shh</p>",
            json!({ "password": "open sesame" }),
        )
        .await;
    let id = f["id"].as_str().unwrap().to_string();
    assert_eq!(f["has_password"], true);

    let anon = browser();
    let (_, v) = app.view(&anon, &id).await;
    assert_eq!(v["state"], "password");
    assert!(v["content_url"].is_null());

    let unlock = |pw: &str, origin: &str| {
        anon.post(app.url(&format!("/api/view/max/{id}/unlock")))
            .header(header::ORIGIN, origin.to_string())
            .json(&json!({ "password": pw }))
    };
    assert_eq!(
        unlock("open sesame", "http://evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN,
        "origin check"
    );
    assert_eq!(
        unlock("wrong", &app.base).send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        unlock("open sesame", &app.base)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let (_, v) = app.view(&anon, &id).await;
    assert_eq!(v["state"], "ok");

    // changing the password invalidates the unlock cookie
    let res = app
        .api()
        .patch(app.url(&format!("/api/files/{id}")))
        .json(&json!({ "password": "new one" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = app.view(&anon, &id).await;
    assert_eq!(v["state"], "password");

    // recipients-only access
    let res = app
        .api()
        .patch(app.url(&format!("/api/files/{id}")))
        .json(&json!({ "password": null, "access": "recipients" }))
        .send()
        .await
        .unwrap();
    let p: Value = res.json().await.unwrap();
    assert_eq!(p["has_password"], false);
    assert_eq!(p["access"], "recipients");
    let (_, v) = app.view(&anon, &id).await;
    assert_eq!(v["state"], "forbidden");
    // the signed-in owner sees their own file, without recording an open
    let owner = browser();
    let res = owner
        .post(app.url("/auth/login"))
        .header(header::ORIGIN, &app.base)
        .json(&json!({ "slug": "max", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = app.view(&owner, &id).await;
    assert_eq!(v["state"], "ok");
    assert!(v["open"].is_null());
    let s: Value = app
        .api()
        .post(app.url(&format!("/api/files/{id}/share")))
        .json(&json!({ "email": "ann@example.com" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        s["recipient"]["name"], "ann",
        "name defaults to the email local part"
    );
    let ann = browser();
    ann.get(s["recipient_link"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    let (_, v) = app.view(&ann, &id).await;
    assert_eq!(v["state"], "ok");

    // email gate on a link-access file creates/reuses recipients
    let g = app
        .upload_json(
            "Lead magnet",
            "lead.html",
            "<p>hi</p>",
            json!({ "email_gate": true }),
        )
        .await;
    let gid = g["id"].as_str().unwrap().to_string();
    let bob = browser();
    let (_, v) = app.view(&bob, &gid).await;
    assert_eq!(v["state"], "email");
    let res = bob
        .post(app.url(&format!("/api/view/max/{gid}/identify")))
        .header(header::ORIGIN, app.base.clone())
        .json(&json!({ "email": "Bob@Example.com", "name": "Bob" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = app.view(&bob, &gid).await;
    assert_eq!(v["state"], "ok");
    assert_eq!(v["recipient"]["name"], "Bob");
    assert_eq!(
        v["sidebar"].as_array().unwrap().len(),
        1,
        "email gate invites to this file"
    );
    let rs: Value = app
        .api()
        .get(app.url("/api/recipients?q=bob"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rs["recipients"][0]["email"], "bob@example.com");
    // Ann (already a recipient) passes the gate automatically
    let (_, v) = app.view(&ann, &gid).await;
    assert_eq!(v["state"], "ok");

    // expiry
    let res = app
        .api()
        .patch(app.url(&format!("/api/files/{gid}")))
        .json(&json!({ "expires_at": "2000-01-01T00:00:00Z" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = app.view(&ann, &gid).await;
    assert_eq!(v["state"], "expired");
    let res = app
        .api()
        .patch(app.url(&format!("/api/files/{gid}")))
        .json(&json!({ "expires_at": "not a date" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = app
        .api()
        .patch(app.url(&format!("/api/files/{gid}")))
        .json(&json!({ "expires_at": null, "access": "off" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = app.view(&ann, &gid).await;
    assert_eq!(v["state"], "off");

    // ambiguous name on share → 409 with candidates
    for e in ["a@x.com", "b@x.com"] {
        let res = app
            .api()
            .post(app.url("/api/recipients"))
            .json(&json!({ "name": "Sam Smith", "email": e }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }
    let res = app
        .api()
        .post(app.url(&format!("/api/files/{id}/share")))
        .json(&json!({ "name": "sam smith" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body: Value = res.json().await.unwrap();
    assert_eq!(
        body["error"]["data"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn oauth_flow() {
    let app = spawn().await;
    let meta: Value = reqwest::get(app.url("/.well-known/oauth-authorization-server"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(meta["token_endpoint"], format!("{}/oauth/token", app.base));
    let prm: Value = reqwest::get(app.url("/.well-known/oauth-protected-resource/mcp"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(prm["resource"], format!("{}/mcp", app.base));

    // unauthenticated MCP → 401 pointing at the resource metadata
    let res = Client::new()
        .post(app.url("/mcp"))
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(
        res.headers()[header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap()
            .contains("/.well-known/oauth-protected-resource")
    );

    // dynamic client registration
    let res = Client::new()
        .post(app.url("/oauth/register"))
        .json(&json!({ "client_name": "Claude", "redirect_uris": ["http://localhost:9999/callback"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let client: Value = res.json().await.unwrap();
    let client_id = client["client_id"].as_str().unwrap().to_string();

    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = handout::ids::pkce_s256(verifier);
    let authz = format!(
        "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri=http%3A%2F%2Flocalhost%3A9999%2Fcallback&code_challenge={challenge}&code_challenge_method=S256&state=xyz&resource={}%2Fmcp",
        handout::oauth::enc(&app.base)
    );

    // no session → bounced to login
    let b = no_redirect_browser();
    let res = b.get(app.url(&authz)).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let loc = res.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(loc.starts_with("/login?next="), "{loc}");

    // wrong password
    let res = b
        .post(app.url("/login"))
        .form(&[("slug", "max"), ("password", "nope"), ("next", &authz)])
        .send()
        .await
        .unwrap();
    assert!(
        res.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .contains("error=1")
    );
    // login
    let res = b
        .post(app.url("/login"))
        .form(&[
            ("slug", "max"),
            ("password", "password123"),
            ("next", &authz),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()[header::LOCATION].to_str().unwrap(), authz);

    // consent page
    let res = b.get(app.url(&authz)).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap();
    assert!(
        csp.contains("form-action 'self' http://localhost:9999;"),
        "{csp}"
    );
    let page = res.text().await.unwrap();
    assert!(page.contains("Claude") && page.contains("Allow"), "{page}");

    // deny
    let res = b
        .post(app.url(&authz))
        .form(&[("decision", "deny")])
        .send()
        .await
        .unwrap();
    assert!(
        res.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .contains("error=access_denied")
    );
    // allow
    let res = b
        .post(app.url(&authz))
        .form(&[("decision", "allow")])
        .send()
        .await
        .unwrap();
    let loc = res.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        loc.starts_with("http://localhost:9999/callback?code="),
        "{loc}"
    );
    assert!(loc.ends_with("&state=xyz"));
    let code = loc
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string();

    // wrong verifier
    let res = Client::new()
        .post(app.url("/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "http://localhost:9999/callback"),
            ("client_id", &client_id),
            (
                "code_verifier",
                "wrong-wrong-wrong-wrong-wrong-wrong-wrong-wrong",
            ),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    // right verifier
    let res = Client::new()
        .post(app.url("/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "http://localhost:9999/callback"),
            ("client_id", &client_id),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let tok: Value = res.json().await.unwrap();
    let access = tok["access_token"].as_str().unwrap().to_string();
    let refresh = tok["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(tok["token_type"], "Bearer");

    // code is single-use
    let res = Client::new()
        .post(app.url("/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "http://localhost:9999/callback"),
            ("client_id", &client_id),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // bearer works on the API
    let me: Value = Client::new()
        .get(app.url("/api/me"))
        .bearer_auth(&access)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["slug"], "max");

    // refresh rotates and revokes the old pair
    let res = Client::new()
        .post(app.url("/oauth/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &client_id),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let tok2: Value = res.json().await.unwrap();
    assert_ne!(tok2["access_token"], access);
    assert_eq!(
        Client::new()
            .get(app.url("/api/me"))
            .bearer_auth(&access)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        Client::new()
            .get(app.url("/api/me"))
            .bearer_auth(tok2["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let res = Client::new()
        .post(app.url("/oauth/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &client_id),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::BAD_REQUEST,
        "old refresh token is dead"
    );
}

/// Minimal MCP client over Streamable HTTP: handles JSON and SSE responses and the session header.
struct Mcp {
    client: Client,
    url: String,
    session: Option<String>,
    next_id: i64,
}

impl Mcp {
    async fn send(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut req = self
            .client
            .post(&self.url)
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-06-18")
            .json(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        if let Some(s) = &self.session {
            req = req.header("Mcp-Session-Id", s);
        }
        let res = req.send().await.unwrap();
        assert!(
            res.status().is_success(),
            "{} {}: {}",
            method,
            res.status(),
            res.text().await.unwrap()
        );
        if let Some(s) = res.headers().get("mcp-session-id") {
            self.session = Some(s.to_str().unwrap().to_string());
        }
        let ct = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = res.text().await.unwrap();
        if ct.starts_with("text/event-stream") {
            for line in body.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let Ok(v) = serde_json::from_str::<Value>(data.trim()) else {
                        continue;
                    };
                    if v["id"] == json!(id) {
                        return v;
                    }
                }
            }
            panic!("no response for id {id} in SSE body: {body}");
        }
        serde_json::from_str(&body).unwrap()
    }
    async fn notify(&self, method: &str) {
        let mut req = self
            .client
            .post(&self.url)
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-06-18")
            .json(&json!({ "jsonrpc": "2.0", "method": method }));
        if let Some(s) = &self.session {
            req = req.header("Mcp-Session-Id", s);
        }
        let res = req.send().await.unwrap();
        assert!(res.status().is_success(), "{method}: {}", res.status());
    }
    async fn call(&mut self, tool: &str, args: Value) -> (bool, Value) {
        let res = self
            .send("tools/call", json!({ "name": tool, "arguments": args }))
            .await;
        assert!(res["error"].is_null(), "protocol error: {res}");
        let r = &res["result"];
        let text = r["content"][0]["text"].as_str().unwrap_or("null");
        (
            r["isError"].as_bool().unwrap_or(false),
            serde_json::from_str(text).unwrap_or(Value::String(text.into())),
        )
    }
}

#[tokio::test]
async fn mcp_tools() {
    let app = spawn().await;
    let mut mcp = Mcp {
        client: app.api(),
        url: app.url("/mcp"),
        session: None,
        next_id: 1,
    };
    let init = mcp
        .send("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }))
        .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "handout", "{init}");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    mcp.notify("notifications/initialized").await;

    let tools = mcp.send("tools/list", json!({})).await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "whoami",
        "upload_file",
        "update_file",
        "set_file",
        "list_files",
        "get_file",
        "delete_file",
        "find_recipients",
        "share_file",
        "revoke_invite",
        "list_opens",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected} in {names:?}"
        );
    }
    let upload = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "upload_file")
        .unwrap();
    assert!(upload["inputSchema"]["properties"]["content"].is_object());

    let (err, me) = mcp.call("whoami", json!({})).await;
    assert!(!err);
    assert_eq!(me["slug"], "max");

    let (err, file) = mcp
        .call(
            "upload_file",
            json!({ "title": "Deck", "filename": "deck.html", "content": "<h1>Deck</h1>" }),
        )
        .await;
    assert!(!err, "{file}");
    let file_id = file["id"].as_str().unwrap().to_string();
    assert!(file_id.starts_with("deck-"));

    let (err, share) = mcp
        .call(
            "share_file",
            json!({ "file_id": file_id, "name": "Jane Roe", "email": "jane@roe.io" }),
        )
        .await;
    assert!(!err, "{share}");
    let link = share["recipient_link"].as_str().unwrap().to_string();

    // the recipient link works end to end
    let jane = browser();
    jane.get(&link).send().await.unwrap();
    let (_, v) = app.view(&jane, &file_id).await;
    assert_eq!(v["state"], "ok");
    assert_eq!(v["recipient"]["name"], "Jane Roe");

    let (err, v2) = mcp
        .call(
            "update_file",
            json!({ "file_id": file_id, "content": "<h1>Deck 2</h1>" }),
        )
        .await;
    assert!(!err, "{v2}");
    assert_eq!(v2["version"], 2);

    let (err, opens) = mcp.call("list_opens", json!({ "file_id": file_id })).await;
    assert!(!err);
    assert_eq!(opens["opens"][0]["recipient"]["name"], "Jane Roe");

    let (err, found) = mcp.call("find_recipients", json!({ "query": "roe" })).await;
    assert!(!err);
    assert_eq!(found["recipients"].as_array().unwrap().len(), 1);

    // tool errors are reported as isError results, not protocol errors
    let (err, missing) = mcp.call("get_file", json!({ "file_id": "nope-000" })).await;
    assert!(err, "{missing}");
    assert_eq!(missing["error"]["code"], "not_found");

    let (err, _) = mcp.call("delete_file", json!({ "file_id": file_id })).await;
    assert!(!err);
    let (_, listed) = mcp.call("list_files", json!({})).await;
    assert_eq!(listed["files"].as_array().unwrap().len(), 0);

    // the owner object is scoped: a second owner's key sees nothing
    let other = service::create_owner(&app.state.db, "other", "Other", "password123")
        .await
        .unwrap();
    let (_, other_key) = service::create_api_key(&app.state, &other, "k")
        .await
        .unwrap();
    let res = Client::new()
        .get(app.url("/api/recipients"))
        .bearer_auth(&other_key)
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["recipients"].as_array().unwrap().len(), 0);
    let _ = Arc::new(app.owner.clone());
}

#[tokio::test]
async fn web_ui_auth() {
    let app = spawn().await;
    let b = browser();
    let origin = app.base.clone();

    // no session → no token
    let res = b
        .post(app.url("/auth/token"))
        .header(header::ORIGIN, &origin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // origin check on login
    let res = b
        .post(app.url("/auth/login"))
        .header(header::ORIGIN, "http://evil.example")
        .json(&json!({ "slug": "max", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // wrong password
    let res = b
        .post(app.url("/auth/login"))
        .header(header::ORIGIN, &origin)
        .json(&json!({ "slug": "max", "password": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // password login → session cookie + web token
    let res = b
        .post(app.url("/auth/login"))
        .header(header::ORIGIN, &origin)
        .json(&json!({ "slug": "max", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["owner"]["slug"], "max");
    let token = body["access_token"].as_str().unwrap().to_string();

    // token works on the bearer-only API; cookies alone do not
    assert_eq!(
        Client::new()
            .get(app.url("/api/files"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        b.get(app.url("/api/files")).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    // session mints fresh tokens (page reload)
    let res = b
        .post(app.url("/auth/token"))
        .header(header::ORIGIN, &origin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let t2 = res.json::<Value>().await.unwrap()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(t2, token);

    // logout revokes session and web tokens
    let res = b
        .post(app.url("/auth/logout"))
        .header(header::ORIGIN, &origin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        b.post(app.url("/auth/token"))
            .header(header::ORIGIN, &origin)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        Client::new()
            .get(app.url("/api/files"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        Client::new()
            .get(app.url("/api/files"))
            .bearer_auth(&t2)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // API-key login (no slug needed), also accepted by the HTML login form
    let res = b
        .post(app.url("/auth/login"))
        .header(header::ORIGIN, &origin)
        .json(&json!({ "api_key": app.key }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let nb = no_redirect_browser();
    let res = nb
        .post(app.url("/login"))
        .form(&[
            ("slug", ""),
            ("password", app.key.as_str()),
            ("next", "/admin"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.headers()[header::LOCATION].to_str().unwrap(), "/admin");
    let res = nb
        .post(app.url("/auth/token"))
        .header(header::ORIGIN, &origin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // landing links to the owner sign-in; a signed-in owner is redirected there
    let landing = reqwest::get(app.url("/"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(landing.contains("href=\"/admin\""));
    let res = nb.get(app.url("/")).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()[header::LOCATION], "/admin");

    // /admin serves the SPA
    let res = reqwest::get(app.url("/admin/recipients")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.text().await.unwrap().contains("<div id=\"app\">"));
}

#[tokio::test]
async fn engagement_heartbeats() {
    let app = spawn().await;
    let f = app
        .upload_json(
            "Long read",
            "read.html",
            "<html><body><h1>Read me</h1><p>…</p></body></html>",
            json!({}),
        )
        .await;
    let id = f["id"].as_str().unwrap().to_string();
    let s: Value = app
        .api()
        .post(app.url(&format!("/api/files/{id}/share")))
        .json(&json!({ "name": "Eve Reader", "email": "eve@x.com" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let eve = browser();
    eve.get(s["recipient_link"].as_str().unwrap())
        .send()
        .await
        .unwrap();

    let (_, v) = app.view(&eve, &id).await;
    assert_eq!(v["state"], "ok");
    let open_id = v["open"]["id"].as_i64().unwrap();
    let open_token = v["open"]["token"].as_str().unwrap().to_string();

    // beacon injected before </body>
    let html = reqwest::get(v["content_url"].as_str().unwrap())
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let script_at = html.find("handout:\"activity\"").expect("beacon present");
    assert!(script_at < html.find("</body>").unwrap());
    assert!(html.ends_with("</body></html>"));

    let hb = |body: Value, origin: &str| {
        eve.post(app.url(&format!("/api/view/max/{id}/heartbeat")))
            .header(header::ORIGIN, origin.to_string())
            .header(header::CONTENT_TYPE, "text/plain")
            .body(body.to_string())
    };
    // origin and token are enforced
    assert_eq!(
        hb(
            json!({ "open_id": open_id, "open_token": open_token, "visible_seconds": 10 }),
            "http://evil.example"
        )
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        hb(
            json!({ "open_id": open_id, "open_token": "nope", "visible_seconds": 10 }),
            &app.base
        )
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        hb(
            json!({ "open_id": open_id + 999, "open_token": open_token, "visible_seconds": 10 }),
            &app.base
        )
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::FORBIDDEN
    );
    // two heartbeats accumulate; deltas are capped at 60s and active ≤ visible
    assert_eq!(hb(json!({ "open_id": open_id, "open_token": open_token, "visible_seconds": 15, "active_seconds": 12, "max_scroll_pct": 40 }), &app.base).send().await.unwrap().status(), StatusCode::NO_CONTENT);
    assert_eq!(hb(json!({ "open_id": open_id, "open_token": open_token, "visible_seconds": 500, "active_seconds": 500, "max_scroll_pct": 85 }), &app.base).send().await.unwrap().status(), StatusCode::NO_CONTENT);

    let opens: Value = app
        .api()
        .get(app.url(&format!("/api/files/{id}/opens")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let o = &opens["opens"][0];
    assert_eq!(o["visible_seconds"], 75);
    assert_eq!(o["active_seconds"], 72);
    assert_eq!(o["max_scroll_pct"], 85);
    assert_eq!(o["recipient"]["name"], "Eve Reader");

    let d: Value = app
        .api()
        .get(app.url(&format!("/api/files/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(d["opens"], 1);
    assert_eq!(d["active_seconds"], 72);
    assert_eq!(d["invites"][0]["active_seconds"], 72);
    assert_eq!(d["invites"][0]["max_scroll_pct"], 85);
    let rid = d["invites"][0]["recipient"]["id"].as_str().unwrap();
    let r: Value = app
        .api()
        .get(app.url(&format!("/api/recipients/{rid}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(r["files"][0]["active_seconds"], 72);
    assert_eq!(r["files"][0]["opens"], 1);
}

#[tokio::test]
async fn crawlers_and_caches() {
    let app = spawn().await;
    let c = browser();
    let content = app.state.cfg.content_url.clone();
    let noindex = "noindex, nofollow, noarchive";

    // every response on both hosts says noindex, including errors and the content fallback
    for url in [
        app.url("/"),
        app.url("/login"),
        app.url("/s/max/nope-0000000000"),
        app.url("/api/view/max/nope-0000000000"),
        format!("{content}/max/nope-0000000000?t=x"),
        format!("{content}/nothing"),
    ] {
        let res = c.get(&url).send().await.unwrap();
        assert_eq!(res.headers()["x-robots-tag"], noindex, "{url}");
    }

    // nothing is cacheable unless a handler says so
    for url in [
        app.url("/login"),
        app.url("/api/view/max/nope-0000000000"),
        format!("{content}/nothing"),
    ] {
        let res = c.get(&url).send().await.unwrap();
        assert_eq!(res.headers()["cache-control"], "no-store", "{url}");
    }
    let res = app.api().get(app.url("/api/me")).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "no-store");
    let res = c
        .get(app.url("/s/max/nope-0000000000"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.headers()["cache-control"],
        "no-cache",
        "SPA keeps its own"
    );

    // robots.txt on both hosts: owner and agent paths blocked, file links left fetchable
    let res = c.get(app.url("/robots.txt")).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "public, max-age=86400");
    let body = res.text().await.unwrap();
    for p in ["/admin", "/api/", "/auth/", "/mcp", "/oauth/"] {
        assert!(body.contains(&format!("Disallow: {p}\n")), "{p}");
    }
    assert!(!body.contains("Disallow: /s"), "file links stay fetchable");
    let res = c.get(format!("{content}/robots.txt")).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.text().await.unwrap().contains("User-agent: *"));
}
