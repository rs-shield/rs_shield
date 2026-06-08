use axum::{
    extract::{Json, State, Request},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
    body::Body,
};
use sqlx::PgPool;
use jsonwebtoken::{decode, DecodingKey, Validation, Header};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use rsb_sdk::crypto::auth::{verify_password, hash_password};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub exp: usize,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

pub async fn login(
    Json(payload): Json<LoginRequest>,
    State(pool): State<PgPool>,
) -> Result<impl IntoResponse, StatusCode> {
    // ... (existing login logic)
    // Reuse SDK verify_password
}

// Full JWT middleware
pub async fn auth_middleware(
    mut req: Request<Body>,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let auth_header = req.headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(t) => t,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "super_secret_change_me_in_prod".to_string());
    let decoding_key = DecodingKey::from_secret(secret.as_ref());

    let claims = match decode::<Claims>(token, &decoding_key, &Validation::default()) {
        Ok(data) => data.claims,
        Err(_) => return Err(StatusCode::UNAUTHORIZED),
    };

    // Optional: add user info to extensions
    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

// Register remains similar, reusing SDK hash_password