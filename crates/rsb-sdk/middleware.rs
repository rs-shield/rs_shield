use crate::auth::handlers::AuthHandlerState;
use axum::{
    extract::{FromRequestParts, State},
    http::{request::Parts, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use rsb_sdk::auth::{JwtManager, SessionClaims, SessionStore};
use std::sync::Arc;
use tracing::{error, warn};

/// Custom extractor to validate JWT from the Authorization header
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub scopes: Vec<String>,
}

#[async_trait::async_trait]
impl<S> FromRequestParts<AuthHandlerState<S>> for AuthenticatedUser
where
    S: SessionStore + Clone + Send + Sync + 'static,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AuthHandlerState<S>,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::unauthorized("Missing Authorization header")),
            ))?;

        if !auth_header.starts_with("Bearer ") {
            //
            //
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::unauthorized("Invalid format")),
            ));
        }

        let token = &auth_header[7..];
        let _claims = state.jwt_manager.verify_token(token).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::unauthorized("Invalid token")),
            )
        })?;

        let jti = state.jwt_manager.extract_jti(token).unwrap_or_default();

        match state.session_store.get(&jti).await {
            Ok(Some(session)) => Ok(AuthenticatedUser {
                user_id: session.user_id,
                scopes: session.scopes,
            }),
            _ => Err((
                StatusCode::UNAUTHORIZED, //
                Json(ErrorResponse::unauthorized("Session expired or revoked")), //
            )),
        }
    }
}

/// Middleware to validate JWT in authenticated requests
pub async fn require_auth_middleware<S: SessionStore + Clone + 'static>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::unauthorized("Missing Authorization header")),
            )
        })?
        .to_string();

    if !auth_header.starts_with("Bearer ") {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::unauthorized(
                "Invalid Authorization header format",
            )),
        ));
    }

    let token = &auth_header[7..];

    // Verificar token JWT
    let claims = state.jwt_manager.verify_token(token).map_err(|e| {
        warn!("JWT verification failed: {:?}", e);
        ( //
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::unauthorized("Invalid or expired token")),
        )
    })?;

    // Verificar se sessão ainda existe
    let jti = state
        .jwt_manager
        .extract_jti(token) //
        .unwrap_or_else(|| "unknown".to_string());

    match state.session_store.get(&jti).await {
        Ok(Some(_session)) => {
            tracing::info!("Request authenticated for user: {}", claims.sub);
            Ok(next.run(request).await)
        }
        Ok(None) => {
            warn!("Session not found or expired for user: {}", claims.sub);
            Err((
                StatusCode::UNAUTHORIZED, //
                Json(ErrorResponse::unauthorized( //
                    "Session expired or revoked. Please authenticate again.",
                )),
            ))
        }
        Err(e) => { //
            error!("Session store error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal_error("Failed to verify session")),
            ))
        }
    }
}

/// Middleware para logar informações da request
pub async fn log_request_middleware(request: Request<axum::body::Body>, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();

    tracing::info!(method = %method, uri = %uri, "Request received");

    next.run(request).await
}

/// Middleware to add security headers
pub async fn security_headers_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;

    // Strict-Transport-Security (HSTS)
    // Forces the browser to use HTTPS for the domain for a long period.
    // max-age=31536000 (1 year), includes subdomains, and preloading.
    // Should be used only in production with HTTPS.
    response.headers_mut().insert(
        "Strict-Transport-Security",
        HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
    );

    // X-Content-Type-Options: nosniff
    // Prevents the browser from "guessing" the MIME type of the content,
    // which can lead to XSS vulnerabilities.
    response.headers_mut().insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );

    // X-Frame-Options: DENY
    // Prevents the page from being embedded in a <frame>, <iframe>, <embed>, or <object>,
    // protecting against clickjacking attacks.
    response
        .headers_mut()
        .insert("X-Frame-Options", HeaderValue::from_static("DENY"));

    // Content-Security-Policy (CSP)
    // Controls which resources the browser is allowed to load.
    // This is a basic example. In production, it should be adjusted for specific domains
    // of scripts, styles, images, etc.
    // 'unsafe-inline' for script-src and style-src is used here due to embedded JS/CSS in the HTML.
    response.headers_mut().insert(
        "Content-Security-Policy",
        HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' http://localhost:3000;"),
    );

    // Referrer-Policy: no-referrer-when-downgrade
    // Controls the amount of referrer information sent with requests.
    response.headers_mut().insert(
        "Referrer-Policy",
        HeaderValue::from_static("no-referrer-when-downgrade"),
    );

    response
}

#[derive(serde::Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

impl ErrorResponse {
    pub fn unauthorized(message: &str) -> Self {
        Self {
            error: "Unauthorized".to_string(),
            message: message.to_string(),
        }
    }

    pub fn forbidden(message: &str) -> Self {
        Self {
            error: "Forbidden".to_string(),
            message: message.to_string(),
        }
    }

    pub fn bad_request(message: &str) -> Self {
        Self {
            error: "Bad Request".to_string(),
            message: message.to_string(),
        }
    }

    pub fn internal_error(message: &str) -> Self {
        Self {
            error: "internal_error".to_string(),
            message: message.to_string(),
        }
    }
}
