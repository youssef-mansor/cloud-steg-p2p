use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chacha20poly1305::{aead::Aead, aead::KeyInit, ChaCha20Poly1305, Key, Nonce};
use openraft::{Raft, RaftMetrics};
use openraft_memstore::TypeConfig;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

pub type NodeId = u64;
pub type RaftNode = Raft<TypeConfig>;

/// Application state shared across HTTP handlers
#[derive(Clone)]
pub struct AppState {
    pub raft: Arc<RaftNode>,
    pub node_id: NodeId,
}

/// Request to initialize the cluster
#[derive(Debug, Serialize, Deserialize)]
pub struct InitRequest {
    pub members: Option<Vec<NodeId>>,
}

/// Request to add a learner node
#[derive(Debug, Serialize, Deserialize)]
pub struct AddLearnerRequest {
    pub node_id: NodeId,
    pub address: String,
}

/// Request to change membership
#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeMembershipRequest {
    pub members: Vec<NodeId>,
}

/// Generic API response
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// Create the HTTP router
pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/metrics", get(metrics))
        .route("/cluster/init", post(init_cluster))
        .route("/cluster/add-learner", post(add_learner))
        .route("/cluster/change-membership", post(change_membership))
        .route("/image/echo", post(echo_image))      // Echo: return same image
        .route("/image/steg", post(steg_image))      // Steganography: embed and return stego image
        .with_state(app_state)
}

/// Root endpoint - show node info
async fn root(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "node_id": state.node_id,
        "status": "running",
        "endpoints": {
            "metrics": "/metrics",
            "init": "POST /cluster/init",
            "add_learner": "POST /cluster/add-learner",
            "change_membership": "POST /cluster/change-membership",
            "image_echo": "POST /image/echo",
            "image_steg": "POST /image/steg"
        }
    }))
}

/// Get Raft metrics
async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    match state.raft.metrics().borrow().clone() {
        metrics => {
            Json(ApiResponse {
                success: true,
                data: Some(serde_json::json!({
                    "node_id": state.node_id,
                    "state": format!("{:?}", metrics.state),
                    "current_term": metrics.current_term,
                    "current_leader": metrics.current_leader,
                    "membership_config": metrics.membership_config,
                })),
                error: None,
            })
        }
    }
}

/// Initialize cluster
async fn init_cluster(
    State(state): State<AppState>,
    Json(_req): Json<InitRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    println!("🎬 Initializing cluster...");
    
    let mut nodes = BTreeMap::new();
    nodes.insert(state.node_id, ());
    
    state.raft.initialize(nodes).await
        .map_err(|e| AppError(format!("Failed to initialize cluster: {}", e)))?;
    
    println!("✅ Cluster initialized! Node {} is now the leader", state.node_id);
    
    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!("Cluster initialized with node {}", state.node_id)),
        error: None,
    }))
}

/// Add a learner node
async fn add_learner(
    State(state): State<AppState>,
    Json(req): Json<AddLearnerRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    println!("📥 Adding learner node {} at {}", req.node_id, req.address);
    
    state.raft.add_learner(req.node_id, (), true).await
        .map_err(|e| AppError(format!("Failed to add learner: {}", e)))?;
    
    println!("✅ Learner node {} added", req.node_id);
    
    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!("Learner {} added", req.node_id)),
        error: None,
    }))
}

/// Change membership
async fn change_membership(
    State(state): State<AppState>,
    Json(req): Json<ChangeMembershipRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    println!("🔄 Changing membership to: {:?}", req.members);
    
    state.raft.change_membership(req.members.clone(), true).await
        .map_err(|e| AppError(format!("Failed to change membership: {}", e)))?;
    
    println!("✅ Membership changed to voters: {:?}", req.members);
    
    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!("Membership changed to: {:?}", req.members)),
        error: None,
    }))
}

/// Echo image - receive and return immediately (no processing)
async fn echo_image(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    println!("📥 Node {} received image for echo: {} bytes", state.node_id, image_size);
    println!("📤 Echoing back {} bytes", image_size);
    
    // Just return the same bytes back (original behavior)
    (
        StatusCode::OK,
        [("Content-Type", "application/octet-stream")],
        body,
    )
}

/// Embed image - receive image and return stego image with secret embedded
async fn steg_image(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    println!("📥 Node {} received image for steganography: {} bytes", state.node_id, image_size);
    println!("🔐 Starting steganography embedding...");
    
    // Check if image is too large (max 1MB for safety)
    if image_size > 1_048_576 {
        println!("❌ Image too large: {} bytes", image_size);
        return (
            StatusCode::BAD_REQUEST,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Image too large (max 1MB), received {} bytes"}}"#, image_size),
        ).into_response();
    }
    
    // Embed the secret image into a cover using steganography
    println!("🔒 Encrypting and embedding image...");
    match embed_image_into_cover(&body[..]) {
        Ok(stego_bytes) => {
            println!("✅ Node {} created stego image: {} bytes (original: {} bytes)", 
                     state.node_id, stego_bytes.len(), image_size);
            (
                StatusCode::OK,
                [("Content-Type", "image/png")],
                axum::body::Bytes::from(stego_bytes),
            ).into_response()
        }
        Err(e) => {
            eprintln!("❌ Node {} failed to embed image: {}", state.node_id, e);
            eprintln!("   Error details: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
            ).into_response()
        }
    }
}

/// Embed secret image bytes into a cover image using steganography
/// Returns PNG bytes of the stego image
fn embed_image_into_cover(secret_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Generate encryption key
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    
    // Encrypt secret using ChaCha20Poly1305
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, secret_bytes)
        .map_err(|e| format!("encrypt failed: {}", e))?;

    // Build payload: [4 bytes len][12 bytes nonce][ciphertext]
    let ct_len = ciphertext.len() as u32;
    let mut payload = Vec::with_capacity(4 + 12 + ciphertext.len());
    payload.extend_from_slice(&ct_len.to_le_bytes());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);

    // Calculate cover size needed (payload bits / 3 bits per pixel)
    let payload_bits = payload.len() * 8;
    let needed_pixels = (payload_bits + 2) / 3; // ceil(bits/3)
    let side = ((needed_pixels as f64).sqrt().ceil() as u32).max(512u32); // min 512x512

    // Create cover image in memory
    let mut cover_img = image::RgbaImage::new(side, side);
    for pixel in cover_img.pixels_mut() {
        *pixel = image::Rgba([180u8, 200u8, 255u8, 255u8]);
    }

    let capacity_bits = (side as usize) * (side as usize) * 3;
    if payload_bits > capacity_bits {
        return Err("Payload too large for cover".into());
    }

    // Convert payload to bits (LSB-first per byte)
    let mut bits = Vec::with_capacity(payload_bits);
    for &b in payload.iter() {
        for i in 0..8 {
            bits.push(((b >> i) & 1u8) != 0);
        }
    }

    // Embed bits into cover using LSB of R, G, B channels
    let (width, height) = cover_img.dimensions();
    let mut raw_bytes = cover_img.into_raw();
    let mut bit_idx = 0usize;
    
    for pixel_chunk in raw_bytes.chunks_exact_mut(4) {
        if bit_idx >= bits.len() {
            break;
        }
        
        // Embed in R channel
        if bit_idx < bits.len() {
            pixel_chunk[0] = (pixel_chunk[0] & 0xFE) | (bits[bit_idx] as u8);
            bit_idx += 1;
        }
        
        // Embed in G channel
        if bit_idx < bits.len() {
            pixel_chunk[1] = (pixel_chunk[1] & 0xFE) | (bits[bit_idx] as u8);
            bit_idx += 1;
        }
        
        // Embed in B channel
        if bit_idx < bits.len() {
            pixel_chunk[2] = (pixel_chunk[2] & 0xFE) | (bits[bit_idx] as u8);
            bit_idx += 1;
        }
        
        // Alpha channel unchanged
    }
    
    // Rebuild image from manipulated raw bytes
    let stego_img = image::RgbaImage::from_raw(width, height, raw_bytes)
        .ok_or("Failed to rebuild image from raw bytes")?;

    // Encode PNG to bytes in memory
    let mut png_bytes = Vec::new();
    image::DynamicImage::ImageRgba8(stego_img)
        .write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageOutputFormat::Png)?;
    
    Ok(png_bytes)
}

/// Error wrapper
struct AppError(String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(self.0),
        });
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}
