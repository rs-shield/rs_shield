use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
    Json,
};
use chrono::Utc;
use rsb_sdk::auth::{
    AuditLogger, AuthRequest, AuthResponse, InMemorySessionStore, JwtManager, RateLimiter, Session,
    SessionStore,
};
use rsb_sdk::credentials::Fido2Manager;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use webauthn_rs::prelude::PublicKeyCredential;

#[derive(Clone)]
pub struct AuthHandlerState<S: SessionStore + Clone> {
    pub jwt_manager: Arc<JwtManager>,
    pub fido2_manager: Arc<Mutex<Fido2Manager>>,
    pub session_store: Arc<S>,
    pub device_flows: Arc<Mutex<HashMap<String, DeviceFlowStatus>>>,
    pub rate_limiter: Arc<RateLimiter>,
    pub audit_logger: Arc<AuditLogger>,
}

#[derive(Clone)]
pub struct DeviceFlowStatus {
    pub user_code: String,
    pub user_id: String,
    pub access_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
const EXPIRE_DATE: i64 = 11800;

fn error_response(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: status.canonical_reason().unwrap_or("Error").to_string(),
            message: message.to_string(),
        }),
    )
}

fn extract_ip_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn extract_user_agent_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// POST /api/auth/start - Iniciar autenticação FIDO2
pub async fn auth_start<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
    Json(req): Json<AuthRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    info!("Auth start: user_id={}", req.user_id);

    if req.user_id.is_empty() {
        warn!("Empty user_id in auth request");
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "user_id is required",
        ));
    }

    // Rate Limiting check
    if !state.rate_limiter.is_allowed(&req.user_id).await {
        state
            .audit_logger
            .log_auth_failure(
                &req.user_id,
                &extract_ip_from_headers(&headers),
                &extract_user_agent_from_headers(&headers),
                "Rate limit exceeded",
            )
            .await;
        return Err(error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many authentication attempts. Please try again later.",
        ));
    }

    let mut fido2_mgr = state.fido2_manager.lock().await;

    match fido2_mgr.start_authentication(&req.user_id) {
        Ok(challenge_response) => {
            let ip = extract_ip_from_headers(&headers);
            info!(
                "Auth challenge created for user: {}, ip: {}",
                req.user_id, ip
            );

            Ok(Json(
                serde_json::to_value(&challenge_response).unwrap_or_default(),
            ))
        }
        Err(rsb_sdk::credentials::fido2::Fido2Error::CredentialNotFound) => {
            warn!("User not found for authentication: {}", req.user_id);
            Err(error_response(
                StatusCode::NOT_FOUND,
                "User not registered. Please register your FIDO2 key first.",
            ))
        }
        Err(e) => {
            warn!("Failed to create auth challenge: {:?}", e);
            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create authentication challenge",
            ))
        }
    }
}

/// POST /api/auth/finish - Completar autenticação FIDO2
pub async fn auth_finish<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Auth finish: user_id={}", req.user_id);

    if req.user_id.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "user_id is required",
        ));
    }

    let credential_json = req
        .fido2_response
        .clone()
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "fido2_response is required"))?;

    let credential: webauthn_rs::prelude::PublicKeyCredential =
        serde_json::from_str(&credential_json)
            .map_err(|_| error_response(StatusCode::BAD_REQUEST, "Invalid credential format"))?;

    let mut fido2_mgr = state.fido2_manager.lock().await;

    match fido2_mgr.finish_authentication(credential) {
        Ok(authenticated_user_id) => {
            // Get counter from credentials
            let counter = fido2_mgr
                .get_credential(&authenticated_user_id)
                .map(|c| c.counter)
                .unwrap_or(0);

            // Create JWT token
            let scopes = vec!["backup".to_string(), "restore".to_string()];
            let token = state
                .jwt_manager
                .create_token(&authenticated_user_id, scopes.clone(), EXPIRE_DATE)
                .map_err(|e| {
                    error!("JWT creation failed: {:?}", e);
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to create access token",
                    )
                })?;

            // Extract JTI and create session
            let jti = state
                .jwt_manager
                .extract_jti(&token)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let session = Session {
                user_id: authenticated_user_id.clone(),
                jti: jti.clone(),
                created_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::seconds(EXPIRE_DATE),
                revoked: false,
                ip_address: Some(extract_ip_from_headers(&headers)),
                user_agent: Some(extract_user_agent_from_headers(&headers)),
                fido2_counter: counter,
                scopes,
            };

            if let Err(e) = state.session_store.save(session).await {
                error!("Failed to save session to store: {}", e);
                return Err(error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to establish session",
                ));
            }

            let ip = extract_ip_from_headers(&headers);
            info!(
                "Auth successful: user_id={}, counter={}, ip={}",
                authenticated_user_id, counter, ip
            );

            // Audit Log
            state
                .audit_logger
                .log_auth_success(
                    &authenticated_user_id,
                    &ip,
                    &extract_user_agent_from_headers(&headers),
                )
                .await;

            // If this is part of a device flow, update the device_flows map
            if let Some(device_code) = req.device_code {
                let mut flows = state.device_flows.lock().await;
                if let Some(flow_status) = flows.get_mut(&device_code) {
                    flow_status.access_token = Some(token.clone());
                    info!(
                        "Device flow for device_code={} updated with access token.",
                        device_code
                    );
                } else {
                    warn!(
                        "Device flow for device_code={} not found during auth_finish.",
                        device_code
                    );
                    return Err(error_response(
                        StatusCode::BAD_REQUEST,
                        "Invalid device_code or user_id not found.",
                    ));
                }
            }

            Ok(Json(AuthResponse {
                access_token: token,
                expires_in: EXPIRE_DATE,
                token_type: "Bearer".to_string(),
                user_id: authenticated_user_id,
            }))
        }
        Err(e) => {
            warn!("FIDO2 verification failed: {:?}", e);
            state
                .audit_logger
                .log_auth_failure(
                    &req.user_id,
                    &extract_ip_from_headers(&headers),
                    &extract_user_agent_from_headers(&headers),
                    &format!("FIDO2 error: {:?}", e),
                )
                .await;
            Err(error_response(
                StatusCode::UNAUTHORIZED,
                "Invalid FIDO2 credential",
            ))
        }
    }
}
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// POST /api/auth/refresh - Refresh access token
pub async fn auth_refresh<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
    Json(req): Json<RefreshTokenRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Token refresh requested");

    let claims = state
        .jwt_manager
        .verify_token(&req.refresh_token)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "Invalid refresh token"))?;

    let user_id = &claims.sub;

    // Create new access token
    let scopes = vec!["backup".to_string(), "restore".to_string()];
    let new_token = state
        .jwt_manager
        .create_token(user_id, scopes.clone(), EXPIRE_DATE)
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create new access token",
            )
        })?;

    // Create new session
    let jti = uuid::Uuid::new_v4().to_string();
    let session = Session {
        user_id: user_id.clone(),
        jti: jti.clone(),
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::seconds(EXPIRE_DATE),
        revoked: false,
        ip_address: Some(extract_ip_from_headers(&headers)),
        user_agent: Some(extract_user_agent_from_headers(&headers)),
        fido2_counter: 0,
        scopes,
    };

    let _ = state.session_store.save(session).await;

    info!("Token refreshed for user: {}", user_id);

    Ok(Json(AuthResponse {
        access_token: new_token,
        expires_in: EXPIRE_DATE,
        token_type: "Bearer".to_string(),
        user_id: user_id.clone(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub access_token: String,
}

/// POST /api/auth/logout - Logout and revoke session
pub async fn auth_logout<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
    Json(req): Json<LogoutRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let claims = state
        .jwt_manager
        .verify_token(&req.access_token)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "Invalid token"))?;

    let jti = state
        .jwt_manager
        .extract_jti(&req.access_token)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let _ = state.session_store.revoke(&jti).await;

    info!("User logged out: {}", claims.sub);

    Ok(Json(json!({
        "message": "Successfully logged out"
    })))
}

/// POST /api/auth/localhost-callback - Callback para CLI via localhost
pub async fn auth_localhost_callback<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    info!("Localhost callback received");

    let user_id = payload
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "user_id required"))?;

    let scopes = vec!["backup".to_string(), "restore".to_string()];
    let token = state
        .jwt_manager
        .create_token(user_id, scopes.clone(), EXPIRE_DATE)
        .map_err(|e| {
            warn!("Failed to create token: {:?}", e);
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token")
        })?;

    let jti = state
        .jwt_manager
        .extract_jti(&token)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let session = Session {
        user_id: user_id.to_string(),
        jti,
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::seconds(EXPIRE_DATE),
        revoked: false,
        ip_address: Some(extract_ip_from_headers(&headers)),
        user_agent: Some(extract_user_agent_from_headers(&headers)),
        fido2_counter: 0,
        scopes,
    };

    let _ = state.session_store.save(session).await;

    info!("Token issued for CLI callback: {}", user_id);

    Ok(Json(json!({
        "access_token": token,
        "expires_in": EXPIRE_DATE,
        "token_type": "Bearer"
    })))
}

/// GET /api/auth/verify - Verificar se token é válido
pub async fn auth_verify<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| {
            error_response(
                StatusCode::UNAUTHORIZED,
                "Missing or invalid authorization header",
            )
        })?;

    let claims = state
        .jwt_manager
        .verify_token(token)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "Invalid token"))?;

    let jti = state
        .jwt_manager
        .extract_jti(token)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    match state.session_store.get(&jti).await {
        Ok(Some(session)) => {
            info!("Token verified for user: {}", session.user_id);
            Ok(Json(json!({
                "valid": true,
                "user_id": session.user_id,
                "scopes": session.scopes,
                "expires_at": session.expires_at
            })))
        }
        Ok(None) => {
            warn!("Session not found or expired for JTI: {}", jti);
            Err(error_response(
                StatusCode::UNAUTHORIZED,
                "Session not found or expired",
            ))
        }
        Err(e) => {
            error!("Session store error: {}", e);
            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to verify session",
            ))
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceFlowStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceFlowTokenRequest {
    pub device_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceFlowTokenResponse {
    pub access_token: Option<String>,
    pub status: String,
    pub expires_in: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceFlowStartRequest {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceFlowLookupRequest {
    pub user_code: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceFlowLookupResponse {
    pub device_code: String,
    pub user_id: String,
}

/// POST /auth/device/start - Iniciar fluxo de autenticação de dispositivo
pub async fn device_flow_start<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    Json(req): Json<DeviceFlowStartRequest>,
) -> Json<DeviceFlowStartResponse> {
    let device_code = uuid::Uuid::new_v4().to_string();
    let user_code = uuid::Uuid::new_v4().to_string()[..6].to_uppercase();

    let mut flows = state.device_flows.lock().await;
    flows.insert(
        device_code.clone(),
        DeviceFlowStatus {
            user_code: user_code.clone(),
            user_id: req.user_id.clone(),
            access_token: None,
        },
    );
    info!(
        "Device flow started: device_code={}, user_code={}",
        device_code, user_code
    );

    Json(DeviceFlowStartResponse {
        device_code: device_code.clone(),
        user_code: user_code.clone(),
        // The user_code is no longer part of the URL, it will be entered manually.
        verification_url: "http://localhost:3000/auth/device/verify".to_string(),
        expires_in: 900, // 15 minutes
        interval: 5,     // Polling interval for CLI
    })
}

/// GET /auth/device/verify - Página de verificação do fluxo de dispositivo
pub async fn device_flow_verify_page<S: SessionStore + Clone>(
    State(_state): State<AuthHandlerState<S>>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Html(r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>RSB Verification</title>
<style>
    body { font-family: sans-serif; display: flex; flex-direction: column; align-items: center; padding: 50px; background: #f5f5f5; }
    .container { background: white; padding: 40px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); max-width: 500px; width: 100%; text-align: center; }
    input[type="text"] { padding: 12px 15px; font-size: 24px; text-align: center; width: calc(100% - 30px); margin-bottom: 15px; border: 2px solid #ccc; border-radius: 5px; letter-spacing: 4px; font-weight: bold; }
    .input-group { margin-bottom: 20px; }
    button { padding: 15px 30px; font-size: 18px; cursor: pointer; background: #3498db; color: white; border: none; border-radius: 5px; width: 100%; transition: background 0.3s; }
    button:hover { background: #2980b9; }
    #authenticateButton { background: #2ecc71; display: none; margin-top: 20px; }
    button:disabled { background: #95a5a6; cursor: not-allowed; }
    .message { margin-top: 20px; padding: 15px; border-radius: 5px; display: none; }
    .error { background: #f8d7da; color: #721c24; }
    .success { background: #d4edda; color: #155724; }
    .info { background: #d1ecf1; color: #0c5460; }
    #userInfo { display: none; text-align: left; margin-top: 20px; padding: 15px; background: #f8f9fa; border-radius: 5px; border: 1px solid #e9ecef; }
</style></head>
<body><div class="container">
    <h1>🔐 Verificação de Dispositivo</h1>
    <p>Insira o código de verificação exibido no seu terminal:</p>
    <div class="input-group" id="lookupSection">
        <input type="text" id="userCodeInput" maxlength="6" placeholder="******" style="text-transform:uppercase;">
        <button id="lookupButton">Confirmar Código</button>
    </div>
    <div id="userInfo">
        <p><strong>Utilizador:</strong> <span id="displayUserId"></span></p>
    </div>
    <button id="authenticateButton">🔐 Autenticar com FIDO2</button>
    <div id="statusMessage" class="message"></div>
</div>
<script type="text/javascript">
    let deviceCode = null;
    let userId = null;
    const statusMessage = document.getElementById('statusMessage');
    function showMessage(text, type) {
        statusMessage.innerText = text;
        statusMessage.className = 'message ' + type;
        statusMessage.style.display = 'block';
    }
    function base64url_to_uint8array(base64url) {
        const padding = '='.repeat((4 - base64url.length % 4) % 4);
        const base64 = (base64url + padding).replace(/-/g, '+').replace(/_/g, '/');
        return Uint8Array.from(atob(base64), c => c.charCodeAt(0));
    }
    function uint8array_to_base64url(buffer) {
        const base64 = btoa(String.fromCharCode(...new Uint8Array(buffer)));
        return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
    }
    document.getElementById('lookupButton').onclick = async () => {
        const userCode = document.getElementById('userCodeInput').value.trim().toUpperCase();
        if (userCode.length !== 6) { showMessage('Insira um código de 6 caracteres.', 'error'); return; }
        try {
            const res = await fetch('/api/auth/device/lookup-code', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ user_code: userCode })
            });
            if (!res.ok) throw new Error('Código inválido ou expirado.');
            const data = await res.json();
            deviceCode = data.device_code;
            userId = data.user_id;
            document.getElementById('displayUserId').innerText = userId;
            document.getElementById('userInfo').style.display = 'block';
            document.getElementById('lookupSection').style.display = 'none';
            document.getElementById('authenticateButton').style.display = 'block';
            showMessage('Código validado! Prossiga com FIDO2.', 'success');
        } catch (e) { showMessage(e.message, 'error'); }
    };
    document.getElementById('authenticateButton').onclick = async () => {
        showMessage('Aguardando chave FIDO2...', 'info');
        try {
            const startRes = await fetch('/api/auth/start', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ user_id: userId })
            });
            const options = await startRes.json();
            const publicKey = options.publicKey || options;
            publicKey.challenge = base64url_to_uint8array(publicKey.challenge);
            if (publicKey.allowCredentials) {
                publicKey.allowCredentials.forEach(c => c.id = base64url_to_uint8array(c.id));
            }
            const credential = await navigator.credentials.get({ publicKey });
            const credentialJson = JSON.stringify({
                id: credential.id, rawId: uint8array_to_base64url(credential.rawId), type: credential.type,
                response: {
                    authenticatorData: uint8array_to_base64url(credential.response.authenticatorData),
                    clientDataJSON: uint8array_to_base64url(credential.response.clientDataJSON),
                    signature: uint8array_to_base64url(credential.response.signature),
                    userHandle: credential.response.userHandle ? uint8array_to_base64url(credential.response.userHandle) : null,
                },
                extensions: credential.getClientExtensionResults(),
            });
            const finishRes = await fetch('/api/auth/finish', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ user_id: userId, fido2_response: credentialJson, device_code: deviceCode })
            });
            if (!finishRes.ok) throw new Error('Falha na autenticação.');
            showMessage('✅ Sucesso! Pode fechar esta página.', 'success');
        } catch (e) { showMessage(e.message, 'error'); }
    };
</script></body></html>"#.to_string()))
}

/// POST /auth/device/token - Polling para obter token após FIDO2 auth
pub async fn device_flow_token<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    Json(req): Json<DeviceFlowTokenRequest>,
) -> Json<DeviceFlowTokenResponse> {
    let flows = state.device_flows.lock().await;

    if let Some(flow) = flows.get(&req.device_code) {
        if let Some(ref token) = flow.access_token {
            return Json(DeviceFlowTokenResponse {
                access_token: Some(token.clone()),
                status: "success".to_string(),
                expires_in: Some(EXPIRE_DATE),
            });
        }
    }

    Json(DeviceFlowTokenResponse {
        access_token: None,
        status: "pending".to_string(),
        expires_in: None,
    })
}

/// POST /api/auth/device/lookup-code - Lookup device flow by user code
pub async fn device_flow_lookup_code<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
    Json(req): Json<DeviceFlowLookupRequest>,
) -> Result<Json<DeviceFlowLookupResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_code = req.user_code.to_uppercase(); // Ensure uppercase for comparison

    let flows = state.device_flows.lock().await;

    // Find the device_code associated with the user_code
    let (device_code, flow_status) = flows
        .iter()
        .find(|(_, flow_status)| flow_status.user_code == user_code)
        .map(|(device_code, flow_status)| (device_code.clone(), flow_status.clone()))
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                "Invalid or expired user code. Please check your terminal.",
            )
        })?;

    info!(
        "User code lookup successful: user_code={}, device_code={}, user_id={}",
        user_code, device_code, flow_status.user_id
    );

    Ok(Json(DeviceFlowLookupResponse {
        device_code,
        user_id: flow_status.user_id,
    }))
}

pub async fn home<S: SessionStore + Clone>(
    State(_state): State<AuthHandlerState<S>>,
) -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>RSB Shield</title>
    <style>
        body { font-family: sans-serif; display: flex; flex-direction: column; align-items: center; padding: 50px; background: #f5f5f5; }
        .container { background: white; padding: 40px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); max-width: 500px; }
        h1 { color: #2c3e50; }
        a { display: inline-block; margin-top: 20px; padding: 10px 20px; background: #3498db; color: white; text-decoration: none; border-radius: 5px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔐 RSB Shield</h1>
        <p>Sistema seguro de backup com autenticação FIDO2</p>
        <a href="/auth/device/start">Iniciar Autenticação</a>
    </div>
</body>
</html>"#,
    )
}

pub async fn device_flow_page<S: SessionStore + Clone>(
    State(_state): State<AuthHandlerState<S>>,
) -> Html<String> {
    Html(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Device Flow</title>
    <style>
        body { font-family: sans-serif; display: flex; flex-direction: column; align-items: center; padding: 50px; background: #f5f5f5; }
        .container { background: white; padding: 40px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); max-width: 500px; }
        .code { font-size: 48px; font-weight: bold; color: #2c3e50; margin: 20px; letter-spacing: 8px; text-align: center; font-family: monospace; }
        .loading { color: #3498db; text-align: center; }
    </style>
</head>
<body>
    <div class="container">
        <h1>✅ Código de Verificação</h1>
        <p>Seu código de verificação:</p>
        <div class="code" id="code" style="display: none;">------</div>
        <div class="loading"><p>Gerando código...</p></div>
    </div>
    <script>
        fetch('/api/auth/device/start', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ user_id: 'cli-user' })
        })
        .then(r => r.json())
        .then(d => {
            const codeDiv = document.getElementById('code');
            codeDiv.innerText = d.user_code;
            codeDiv.style.display = 'block';
            document.querySelector('.loading').style.display = 'none';
            console.log('Device code:', d.device_code);
            console.log('User code:', d.user_code);
            console.log('Verification URL:', d.verification_url);
        })
        .catch(e => {
            console.error('Error:', e);
            document.querySelector('.loading').innerText = 'Erro ao gerar código';
        });
    </script>
</body>
</html>"#.to_string())
}

pub async fn device_flow_start_api<S: SessionStore + Clone>(
    State(state): State<AuthHandlerState<S>>,
) -> Json<DeviceFlowStartResponse> {
    let device_code = uuid::Uuid::new_v4().to_string();
    let user_code = uuid::Uuid::new_v4().to_string()[..6].to_uppercase();

    let mut flows = state.device_flows.lock().await;
    flows.insert(
        device_code.clone(),
        DeviceFlowStatus {
            user_code: user_code.clone(),
            user_id: "cli-user".to_string(),
            access_token: None,
        },
    );

    Json(DeviceFlowStartResponse {
        device_code: device_code.clone(),
        user_code: user_code.clone(),
        verification_url: format!(
            "http://localhost:3000/auth/device/verify?user_code={}",
            user_code
        ),
        expires_in: 900,
        interval: 5,
    })
}
