use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::server::AppState;

#[derive(askama::Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

pub fn render_login(error: Option<String>, status: StatusCode) -> axum::response::Response {
    let tmpl = LoginTemplate { error };
    match askama::Template::render(&tmpl) {
        Ok(html) => (status, axum::response::Html(html)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response(),
    }
}

pub async fn login_form() -> impl IntoResponse {
    render_login(None, StatusCode::OK)
}

pub async fn handle_login(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let token = form.token.trim().to_string();

    // No admin_token set? Auth is effectively disabled -- let them through.
    let expected = match &state.config.server.admin_token {
        Some(t) => t,
        None => {
            return axum::response::Redirect::to("/web/projects/").into_response();
        }
    };

    if subtle::ConstantTimeEq::ct_eq(token.as_bytes(), expected.as_bytes()).into() {
        let secure = state
            .config
            .server
            .external_url
            .as_ref()
            .is_some_and(|u| u.starts_with("https://"));
        let secure_flag = if secure { "; Secure" } else { "" };
        let cookie =
            format!("stackpit_token={token}; Path=/; SameSite=Strict; HttpOnly{secure_flag}");
        let mut resp = axum::response::Redirect::to("/web/projects/").into_response();
        if let Ok(val) = cookie.parse() {
            resp.headers_mut().insert("set-cookie", val);
        }
        resp
    } else {
        render_login(Some("Invalid token".to_string()), StatusCode::UNAUTHORIZED)
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    token: String,
}
