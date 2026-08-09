use axum::{
    extract::{FromRef, Request, State},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::cookie::{Cookie, Key, SameSite};
use axum_extra::extract::SignedCookieJar;
use sqlx::SqlitePool;

const SESSION_COOKIE: &str = "ferrite_session";
const SESSION_DURATION_SECS: i64 = 60 * 60 * 24 * 30; // 30 days
const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub http_client: reqwest::Client,
    pub site_key: std::sync::Arc<str>,
    pub secret_key: std::sync::Arc<str>,
    pub cookie_key: Key,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let site_key = std::env::var("TURNSTILE_SITE_KEY")
            .expect("TURNSTILE_SITE_KEY must be set in .env");
        let secret_key = std::env::var("TURNSTILE_SECRET_KEY")
            .expect("TURNSTILE_SECRET_KEY must be set in .env");

        // Deriving the cookie-signing key from the Turnstile secret means sessions
        // survive a server restart (e.g. the auto-deploy on every push) without
        // needing a separate signing-key secret in .env.
        let cookie_key = Key::derive_from(secret_key.as_bytes());

        Self {
            pool,
            http_client: reqwest::Client::new(),
            site_key: site_key.into(),
            secret_key: secret_key.into(),
            cookie_key,
        }
    }
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn has_valid_session(jar: &SignedCookieJar) -> bool {
    let Some(cookie) = jar.get(SESSION_COOKIE) else {
        return false;
    };
    let Ok(issued_at) = cookie.value().parse::<i64>() else {
        return false;
    };
    let now = now_unix();
    // issued_at is signed, so it can't be forged, but we still bound how far in
    // the future it can be to tolerate clock skew rather than trusting it blindly.
    now >= issued_at && now - issued_at < SESSION_DURATION_SECS
}

fn session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, now_unix().to_string()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(SESSION_DURATION_SECS))
        .build()
}

/// Gates every request behind a valid signed session cookie. Requests without
/// one get served the Turnstile challenge page instead of reaching the app.
pub async fn gate(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    req: Request,
    next: Next,
) -> Response {
    if has_valid_session(&jar) {
        return next.run(req).await;
    }

    let template = std::fs::read_to_string("frontend/turnstile.html")
        .unwrap_or_else(|_| "<h1>Could not load challenge page</h1>".to_string());
    Html(template.replace("{{SITE_KEY}}", &state.site_key)).into_response()
}

#[derive(serde::Deserialize)]
pub struct VerifyForm {
    #[serde(rename = "cf-turnstile-response")]
    token: String,
}

#[derive(serde::Deserialize)]
struct SiteVerifyResponse {
    success: bool,
}

async fn check_with_cloudflare(client: &reqwest::Client, secret: &str, token: &str) -> bool {
    let result = client
        .post(SITEVERIFY_URL)
        .form(&[("secret", secret), ("response", token)])
        .send()
        .await;

    match result {
        Ok(resp) => resp
            .json::<SiteVerifyResponse>()
            .await
            .map(|body| body.success)
            .unwrap_or(false),
        Err(_) => false,
    }
}

pub async fn verify(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Form(form): Form<VerifyForm>,
) -> impl IntoResponse {
    let ok = check_with_cloudflare(&state.http_client, &state.secret_key, &form.token).await;

    let jar = if ok {
        jar.add(session_cookie())
    } else {
        jar
    };

    (jar, Redirect::to("/"))
}
