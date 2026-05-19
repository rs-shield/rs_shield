use axum::response::Html;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use crate::credentials::Fido2Manager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use webauthn_rs::prelude::*;

pub type AppState = Arc<Mutex<Fido2Manager>>;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
}

#[derive(Deserialize)]
pub struct AuthRequest {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct CredentialInfo {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub counter: u32,
    pub created_at: String,
    pub last_used: Option<String>,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

// Custom error response
pub struct ApiResponse<T: Serialize>(pub T, pub StatusCode);

impl<T: Serialize + Send + 'static> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (self.1, Json(self.0)).into_response()
    }
}

pub fn create_router(state: AppState, html: Html<&'static str>) -> Router {
    Router::new()
        .route("/", get(move || async move { html }))
        // register
        .route("/register/start", post(register_start))
        .route("/register/finish", post(register_finish))
        // auth
        .route("/auth/start", post(auth_start))
        .route("/auth/finish", post(auth_finish))
        // credentials management
        .route("/credentials", get(list_credentials))
        .route("/credentials/:user_id", delete(delete_credential))
        .with_state(state)
}

async fn index(html: Html<&'static str>) -> Html<&'static str> {
    html
}

// ================= REGISTER =================

async fn register_start(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<CreationChallengeResponse>, (StatusCode, Json<ApiError>)> {
    info!("Register start: user_id={}", req.user_id);

    // Validate request
    if req.user_id.is_empty() || req.user_id.len() > 255 {
        warn!("Invalid user_id in register request");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Invalid user_id length".to_string(),
            }),
        ));
    }

    let mut m = state.lock().await;

    m.start_registration(&req.user_id, &req.username, &req.display_name)
        .map(Json)
        .map_err(|e| {
            error!("Registration start error: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: e.to_string(),
                }),
            )
        })
}

async fn register_finish(
    State(state): State<AppState>,
    Json(res): Json<RegisterPublicKeyCredential>,
) -> Result<Json<bool>, (StatusCode, Json<ApiError>)> {
    let mut m = state.lock().await;

    match m.finish_registration(res) {
        Ok(_) => {
            // 🔐 save credentials to file
            let path = Fido2Manager::default_storage_path().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError { error: e }),
                )
            })?;

            m.save_to_file(&path).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: e.to_string(),
                    }),
                )
            })?;

            Ok(Json(true))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: e.to_string(),
            }),
        )),
    }
}

// ================= AUTH =================

async fn auth_start(
    State(state): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<RequestChallengeResponse>, (StatusCode, Json<ApiError>)> {
    info!("Auth start: user_id={}", req.user_id);

    if req.user_id.is_empty() {
        warn!("Empty user_id in auth request");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "user_id is required".to_string(),
            }),
        ));
    }

    let mut m = state.lock().await;

    m.start_authentication(&req.user_id).map(Json).map_err(|e| {
        error!("Auth start error: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: e.to_string(),
            }),
        )
    })
}

async fn auth_finish(
    State(state): State<AppState>,
    Json(res): Json<PublicKeyCredential>,
) -> Result<Json<String>, (StatusCode, Json<ApiError>)> {
    info!("Auth finish");
    let mut m = state.lock().await;

    m.finish_authentication(res).map(Json).map_err(|e| {
        error!("Auth finish error: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: e.to_string(),
            }),
        )
    })
}

// ================= CREDENTIALS MANAGEMENT =================

async fn list_credentials(
    State(state): State<AppState>,
) -> Result<Json<Vec<CredentialInfo>>, (StatusCode, Json<ApiError>)> {
    info!("Listing credentials");
    let m = state.lock().await;

    let credentials = m
        .list_credentials()
        .into_iter()
        .map(|c| CredentialInfo {
            user_id: c.user_id,
            username: c.user_name,
            display_name: c.display_name,
            counter: c.counter,
            created_at: c.created_at,
            last_used: c.last_used,
        })
        .collect();

    Ok(Json(credentials))
}

async fn delete_credential(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<bool>, (StatusCode, Json<ApiError>)> {
    info!("Deleting credential: user_id={}", user_id);

    if user_id.is_empty() {
        warn!("Empty user_id in delete request");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "user_id is required".to_string(),
            }),
        ));
    }

    let mut m = state.lock().await;

    m.revoke_user(&user_id)
        .map(|_| Json(true))
        .map_err(|e| {
            error!("Delete credential error: {}", e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(ApiError {
                    error: e.to_string(),
                }),
            )
        })
}

// ================= SERVER =================

pub async fn run_server(
    manager: Arc<Mutex<Fido2Manager>>,
    html: Html<&'static str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let router = create_router(manager,  html);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    let url = "http://localhost:3000";

    info!("🌐 Server running at {}", url);
    println!("🌐 Server: {}", url);

    // abre browser
    let _ = open::that(url);

    axum::serve(listener, router).await?;

    Ok(())
}
