use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use tower_http::limit::RequestBodyLimitLayer;
use openraft::{Raft, ServerState};
use openraft_memstore::TypeConfig;
use rand::random;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use png as png_crate;

pub type NodeId = u64;
pub type RaftNode = Raft<TypeConfig>;


/// Application state shared across HTTP handlers
#[derive(Clone)]
pub struct AppState {
    pub raft: Arc<RaftNode>,
    pub node_id: NodeId,
    pub http_addresses: Arc<RwLock<BTreeMap<NodeId, String>>>,
    pub self_http_addr: String,
    pub healthy_nodes: Arc<RwLock<BTreeMap<NodeId, bool>>>, // Track which nodes are healthy
}


// use crate::single_node_monitor;  // unused import; functions referenced directly via crate::single_node_monitor::

/// Start background monitoring tasks for single-node operation
pub fn start_monitors(state: AppState) {
    let state_arc = Arc::new(state.clone());
    
    // Start single-node monitor
    let state_clone = Arc::clone(&state_arc);
    tokio::spawn(async move {
        crate::single_node_monitor::monitor_single_node(state_clone).await;
    });
    
    // Start new-node monitor
    let state_clone = Arc::clone(&state_arc);
    tokio::spawn(async move {
        crate::single_node_monitor::monitor_new_nodes(state_clone).await;
    });
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
        .route("/image/steg", post(steg_image_multipart))      // Steganography: embed and return stego image (multipart)
        .route("/image/extract", post(extract_image))          // Extract secret from stego image
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // axum extractor limit
        .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024)) // hyper/tower hard cap
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

/// Forward HTTP request to another node
async fn forward_request_to_node(
    node_id: NodeId,
    http_addr: &str,
    path: &str,
    body: &[u8],
    content_type: Option<&str>,
) -> Result<reqwest::Response, String> {
    let url = format!("http://{}{}", http_addr, path);
    // Create client with timeout to avoid hanging on crashed nodes
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut req = client.post(&url).header("X-Raft-Forwarded", "true");
    if let Some(ct) = content_type { req = req.header("Content-Type", ct); }
    req.body(body.to_vec())
        .send()
        .await
        .map_err(|e| format!("Failed to forward to node {} at {}: {}", node_id, http_addr, e))
}

/// Get a random node ID from available nodes (including self)
/// Prefers healthy nodes, but gives unhealthy nodes a 10% chance for recovery
async fn get_random_node(state: &AppState) -> Option<(NodeId, String)> {
    let http_addrs = state.http_addresses.read().await;
    let healthy = state.healthy_nodes.read().await;
    
    if http_addrs.is_empty() {
        // Fallback to self (self is always considered healthy)
        return Some((state.node_id, state.self_http_addr.clone()));
    }
    
    // Separate healthy and unhealthy nodes
    let mut healthy_node_ids: Vec<_> = Vec::new();
    let mut unhealthy_node_ids: Vec<_> = Vec::new();
    
    for (id, _) in http_addrs.iter() {
        if *id == state.node_id {
            // Self is always healthy
            healthy_node_ids.push(*id);
        } else if healthy.get(id).copied().unwrap_or(false) {
            healthy_node_ids.push(*id);
        } else {
            unhealthy_node_ids.push(*id);
        }
    }
    
    // If we have healthy nodes and random check (20% chance), give unhealthy nodes a chance
    // This allows crashed nodes to prove they've recovered
    let use_unhealthy = !healthy_node_ids.is_empty() && 
                        !unhealthy_node_ids.is_empty() && 
                        (random::<usize>() % 5) == 0; // 20% chance (1 in 5)
    
    if use_unhealthy && !unhealthy_node_ids.is_empty() {
        // Give an unhealthy node a chance to prove it's back
        let idx = random::<usize>() % unhealthy_node_ids.len();
        let selected_id = unhealthy_node_ids[idx];
        let addr = http_addrs.get(&selected_id)?.clone();
        println!("🔍 Probation: trying unhealthy node {} to check if it recovered", selected_id);
        return Some((selected_id, addr));
    }
    
    // Normal case: select from healthy nodes
    if healthy_node_ids.is_empty() {
        // No healthy nodes, try unhealthy ones
        if unhealthy_node_ids.is_empty() {
            return Some((state.node_id, state.self_http_addr.clone()));
        }
        let idx = random::<usize>() % unhealthy_node_ids.len();
        let selected_id = unhealthy_node_ids[idx];
        let addr = http_addrs.get(&selected_id)?.clone();
        return Some((selected_id, addr));
    }
    
    let idx = random::<usize>() % healthy_node_ids.len();
    let selected_id = healthy_node_ids[idx];
    let addr = http_addrs.get(&selected_id)?.clone();
    
    Some((selected_id, addr))
}

/// Echo image - receive and return immediately (no processing)
/// Load balancing: Followers reject direct requests, Leader forwards randomly
async fn echo_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    
    // Check if this is a forwarded request from the leader
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    
    // Check if we're the leader
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    
    // Followers only accept forwarded requests, not direct client requests
    if !is_leader && !is_forwarded {
        println!("🚫 Node {} (follower) dropping direct echo request - only leader processes direct requests", state.node_id);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
        ).into_response();
    }
    
    // If forwarded request and we're a follower, process it
    if !is_leader && is_forwarded {
        println!("📥 Node {} (follower) processing forwarded echo request: {} bytes", state.node_id, image_size);
        println!("📤 Echoing back {} bytes", image_size);
        return (
            StatusCode::OK,
            [
                ("Content-Type", "application/octet-stream"),
                ("X-Processed-By-Node", &format!("{}", state.node_id)),
            ],
            body,
        ).into_response();
    }
    
    // Leader: randomly assign to a node (including self)
    let (target_id, target_addr) = match get_random_node(&state).await {
        Some(addr) => addr,
        None => {
            println!("⚠️  No available nodes for forwarding, processing locally");
            (state.node_id, state.self_http_addr.clone())
        }
    };
    
    if target_id == state.node_id {
        // Process locally
        println!("📥 Node {} (leader) processing echo request locally: {} bytes", state.node_id, image_size);
        println!("📤 Echoing back {} bytes", image_size);
        (
            StatusCode::OK,
            [
                ("Content-Type", "application/octet-stream"),
                ("X-Processed-By-Node", &format!("{}", state.node_id)),
            ],
            body,
        ).into_response()
    } else {
        // Forward to selected node with retry logic
        println!("🔄 Node {} (leader) forwarding echo request to node {} at {}", 
                state.node_id, target_id, target_addr);
        match forward_request_to_node(target_id, &target_addr, "/image/echo", &body, None).await {
            Ok(response) => {
                match response.bytes().await {
                    Ok(bytes) => {
                        println!("✅ Received response from node {}: {} bytes", target_id, bytes.len());
                        // Mark node as healthy on successful response
                        let mut healthy = state.healthy_nodes.write().await;
                        healthy.insert(target_id, true);
                        drop(healthy);
                        
                        let mut headers = HeaderMap::new();
                        headers.insert("Content-Type", "application/octet-stream".parse().unwrap());
                        headers.insert("X-Processed-By-Node", format!("{}", target_id).parse().unwrap());
                        (StatusCode::OK, headers, bytes).into_response()
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to read response from node {}: {}", target_id, e);
                        // Mark node as unhealthy
                        let mut healthy = state.healthy_nodes.write().await;
                        healthy.insert(target_id, false);
                        drop(healthy);
                        
                        // Return error - client should retry
                        (
                            StatusCode::BAD_GATEWAY,
                            [("Content-Type", "application/json")],
                            format!(r#"{{"error": "Failed to read response from node {}"}}"#, target_id),
                        ).into_response()
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to forward to node {}: {} - marking as unhealthy", target_id, e);
                // Mark node as unhealthy when forwarding fails
                let mut healthy = state.healthy_nodes.write().await;
                healthy.insert(target_id, false);
                drop(healthy);
                
                // Return error - client should retry (multicast again)
                (
                    StatusCode::BAD_GATEWAY,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to forward to node {}: {}"}}"#, target_id, e),
                ).into_response()
            }
        }
    }
}

/// Embed image - receive image and return stego image with secret embedded
/// Load balancing: Followers reject direct requests, Leader forwards randomly
async fn steg_image_multipart(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Parse multipart safely: expect fields 'cover' and 'secret'
    let mut cover: Option<Vec<u8>> = None;
    let mut secret: Option<Vec<u8>> = None;
    let mut secret_mime: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().map(|s| s.to_string()).unwrap_or_default();
        let fct = field.content_type().map(|m| m.to_string());
        let bytes = match field.bytes().await {
            Ok(b) => b.to_vec(),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to read multipart field: {}"}}"#, e),
                ).into_response();
            }
        };
        if name == "cover" { cover = Some(bytes); }
        else if name == "secret" { secret = Some(bytes); secret_mime = fct; }
    }

    let cover = match cover { Some(c) => c, None => return (
        StatusCode::BAD_REQUEST,
        [("Content-Type", "application/json")],
        "Missing 'cover' part".to_string(),
    ).into_response() };
    let secret = match secret { Some(s) => s, None => return (
        StatusCode::BAD_REQUEST,
        [("Content-Type", "application/json")],
        "Missing 'secret' part".to_string(),
    ).into_response() };

    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    let is_forwarded = headers.contains_key("x-raft-forwarded");

    // Followers only accept forwarded requests, except in single-node mode
    if !is_leader && is_forwarded {
        let node_id = state.node_id;
        let sm = secret_mime.clone();
        let res = tokio::task::spawn_blocking(move || embed_cover_with_secret_chunk(&cover, &secret, sm.as_deref())).await;
        return match res {
            Ok(Ok(stego_bytes)) => (
                StatusCode::OK,
                [("Content-Type", "image/png"), ("X-Processed-By-Node", &format!("{}", node_id))],
                axum::body::Bytes::from(stego_bytes),
            ).into_response(),
            Ok(Err(e)) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
            ).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                "Task execution failed".to_string(),
            ).into_response(),
        };
    } else if !is_leader && !is_forwarded {
        // Single-node mode: allow direct processing if only self is configured
        let http_addrs_len = {
            let map = state.http_addresses.read().await;
            map.len()
        };
        if http_addrs_len == 1 {
            let node_id = state.node_id;
            let sm = secret_mime.clone();
            let res = tokio::task::spawn_blocking(move || embed_cover_with_secret_chunk(&cover, &secret, sm.as_deref())).await;
            return match res {
                Ok(Ok(stego_bytes)) => (
                    StatusCode::OK,
                    [("Content-Type", "image/png"), ("X-Processed-By-Node", &format!("{}", node_id))],
                    axum::body::Bytes::from(stego_bytes),
                ).into_response(),
                Ok(Err(e)) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
                ).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    "Task execution failed".to_string(),
                ).into_response(),
            };
        } else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
            ).into_response();
        }
    }

    // Leader: forward to a randomly selected node (including self)
    let (target_id, target_addr) = match get_random_node(&state).await { Some(v) => v, None => (state.node_id, state.self_http_addr.clone()) };
    if target_id == state.node_id {
        // Process locally
        let node_id = state.node_id;
        let sm = secret_mime.clone();
        let res = tokio::task::spawn_blocking(move || embed_cover_with_secret_chunk(&cover, &secret, sm.as_deref())).await;
        return match res {
            Ok(Ok(stego_bytes)) => (
                StatusCode::OK,
                [("Content-Type", "image/png"), ("X-Processed-By-Node", &format!("{}", node_id))],
                axum::body::Bytes::from(stego_bytes),
            ).into_response(),
            Ok(Err(e)) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
            ).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                "Task execution failed".to_string(),
            ).into_response(),
        };
    }

    // Build multipart body manually (to avoid enabling reqwest multipart feature)
    let boundary = format!("----stegBoundary{:x}", rand::random::<u64>());
    let ct_header = format!("multipart/form-data; boundary={}", boundary);
    let mut body_bytes: Vec<u8> = Vec::with_capacity(cover.len() + secret.len() + 512);
    let crlf = b"\r\n";

    // Part: cover
    body_bytes.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"cover\"; filename=\"cover.png\"\r\n");
    body_bytes.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body_bytes.extend_from_slice(&cover);
    body_bytes.extend_from_slice(crlf);

    // Part: secret
    let sec_mime = secret_mime.as_deref().unwrap_or("application/octet-stream");
    body_bytes.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"secret\"; filename=\"secret\"\r\n");
    body_bytes.extend_from_slice(format!("Content-Type: {}\r\n\r\n", sec_mime).as_bytes());
    body_bytes.extend_from_slice(&secret);
    body_bytes.extend_from_slice(crlf);

    // Closing boundary
    body_bytes.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    match forward_request_to_node(target_id, &target_addr, "/image/steg", &body_bytes, Some(&ct_header)).await {
        Ok(r) => {
            let processed_by = r.headers().get("x-processed-by-node").and_then(|h| h.to_str().ok()).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", target_id));
            match r.bytes().await {
                Ok(bytes) => {
                    // Mark node healthy
                    let mut healthy = state.healthy_nodes.write().await;
                    healthy.insert(target_id, true);
                    drop(healthy);
                    let mut h = HeaderMap::new();
                    h.insert("Content-Type", "image/png".parse().unwrap());
                    h.insert("X-Processed-By-Node", processed_by.parse().unwrap());
                    (StatusCode::OK, h, bytes).into_response()
                }
                Err(_e) => {
                    // Mark node unhealthy and process locally as fallback
                    let mut healthy = state.healthy_nodes.write().await;
                    healthy.insert(target_id, false);
                    drop(healthy);
                    let node_id = state.node_id;
                    let sm = secret_mime.clone();
                    let res = tokio::task::spawn_blocking(move || embed_cover_with_secret_chunk(&cover, &secret, sm.as_deref())).await;
                    return match res {
                        Ok(Ok(stego_bytes)) => (
                            StatusCode::OK,
                            [("Content-Type", "image/png"), ("X-Processed-By-Node", &format!("{}", node_id))],
                            axum::body::Bytes::from(stego_bytes),
                        ).into_response(),
                        Ok(Err(e)) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            [("Content-Type", "application/json")],
                            format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
                        ).into_response(),
                        Err(_) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            [("Content-Type", "application/json")],
                            "Task execution failed".to_string(),
                        ).into_response(),
                    };
                }
            }
        }
        Err(_e) => {
            // Mark node unhealthy and process locally as fallback
            let mut healthy = state.healthy_nodes.write().await;
            healthy.insert(target_id, false);
            drop(healthy);
            let node_id = state.node_id;
            let sm = secret_mime.clone();
            let res = tokio::task::spawn_blocking(move || embed_cover_with_secret_chunk(&cover, &secret, sm.as_deref())).await;
            match res {
                Ok(Ok(stego_bytes)) => (
                    StatusCode::OK,
                    [("Content-Type", "image/png"), ("X-Processed-By-Node", &format!("{}", node_id))],
                    axum::body::Bytes::from(stego_bytes),
                ).into_response(),
                Ok(Err(e)) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
                ).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    "Task execution failed".to_string(),
                ).into_response(),
            }
        }
    }
}

/// Embed secret image bytes into a cover image by adding a custom PNG chunk 'stEg'
/// The pixel data of the cover is preserved; the secret is stored losslessly.
fn embed_cover_with_secret_chunk(
    cover_bytes: &[u8],
    secret_bytes: &[u8],
    secret_mime: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Re-encode cover as PNG without modifying pixels
    let cover_img = image::load_from_memory(cover_bytes)?.to_rgba8();
    let (w, h) = cover_img.dimensions();
    let raw = cover_img.into_raw();

    // Payload: magic "STG2" | mime_len u16 | mime bytes | data_len u32 | data bytes
    let mime = secret_mime.unwrap_or("application/octet-stream").as_bytes();
    let mut payload = Vec::with_capacity(4 + 2 + mime.len() + 4 + secret_bytes.len());
    payload.extend_from_slice(b"STG2");
    payload.extend_from_slice(&(mime.len() as u16).to_le_bytes());
    payload.extend_from_slice(mime);
    payload.extend_from_slice(&(secret_bytes.len() as u32).to_le_bytes());
    payload.extend_from_slice(secret_bytes);

    let mut out = Vec::new();
    let mut encoder = png_crate::Encoder::new(&mut out, w, h);
    encoder.set_color(png_crate::ColorType::Rgba);
    encoder.set_depth(png_crate::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&raw)?;
    // Write custom ancillary chunk 'stEg' (ancillary + safe-to-copy)
    writer.write_chunk(png_crate::chunk::ChunkType(*b"stEg"), &payload)?;
    writer.finish()?;
    Ok(out)
}

/// Extract a secret from the custom PNG chunk 'stEg'. Returns (bytes, mime)
fn extract_secret_from_stego(stego_bytes: &[u8]) -> Result<(Vec<u8>, String), Box<dyn std::error::Error + Send + Sync>> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if stego_bytes.len() < 8 || &stego_bytes[0..8] != SIG { return Err("Not a PNG".into()); }
    let mut i = 8usize;
    while i + 12 <= stego_bytes.len() {
        let len = u32::from_be_bytes(stego_bytes[i..i+4].try_into().unwrap()) as usize; i += 4;
        let ctype = &stego_bytes[i..i+4]; i += 4;
        if ctype == b"stEg" {
            if i + len + 4 > stego_bytes.len() { return Err("Chunk truncated".into()); }
            let data = &stego_bytes[i..i+len];
            // Parse payload: magic STG2 | mime_len u16 | mime | data_len u32 | data
            if data.len() < 4+2+4 { return Err("Payload too small".into()); }
            if &data[0..4] != b"STG2" { return Err("Wrong payload magic".into()); }
            let ml = u16::from_le_bytes([data[4],data[5]]) as usize;
            if data.len() < 6 + ml + 4 { return Err("Payload malformed".into()); }
            let mime = std::str::from_utf8(&data[6..6+ml]).unwrap_or("application/octet-stream").to_string();
            let dl_off = 6+ml;
            let dlen = u32::from_le_bytes([data[dl_off],data[dl_off+1],data[dl_off+2],data[dl_off+3]]) as usize;
            if data.len() < dl_off+4 + dlen { return Err("Payload data truncated".into()); }
            let content = data[dl_off+4 .. dl_off+4+dlen].to_vec();
            return Ok((content, mime));
        } else {
            // skip data + CRC
            i += len + 4; // data + CRC
        }
    }
    Err("stEg chunk not found".into())
}

/// Extract handler: receive stego PNG bytes, return extracted secret with original MIME
async fn extract_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);

    // Followers only accept forwarded requests, except in single-node mode
    if !is_leader && is_forwarded {
        let node_id = state.node_id;
        let bytes = body.to_vec();
        let res = tokio::task::spawn_blocking(move || extract_secret_from_stego(&bytes)).await;
        return match res {
            Ok(Ok((content, mime))) => {
                let mut h = HeaderMap::new();
                h.insert("Content-Type", mime.parse().unwrap_or("application/octet-stream".parse().unwrap()));
                h.insert("X-Processed-By-Node", format!("{}", node_id).parse().unwrap());
                (StatusCode::OK, h, axum::body::Bytes::from(content)).into_response()
            }
            Ok(Err(e)) => (
                StatusCode::BAD_REQUEST,
                [("Content-Type", "application/json")],
                format!(r#"{{"error":"Failed to extract: {}"}}"#, e),
            ).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                "Task execution failed".to_string(),
            ).into_response(),
        };
    } else if !is_leader && !is_forwarded {
        // Single-node mode: allow direct requests if only self is configured
        let http_addrs_len = {
            let map = state.http_addresses.read().await;
            map.len()
        };
        if http_addrs_len == 1 {
            let node_id = state.node_id;
            let bytes = body.to_vec();
            let res = tokio::task::spawn_blocking(move || extract_secret_from_stego(&bytes)).await;
            return match res {
                Ok(Ok((content, mime))) => {
                    let mut h = HeaderMap::new();
                    h.insert("Content-Type", mime.parse().unwrap_or("application/octet-stream".parse().unwrap()));
                    h.insert("X-Processed-By-Node", format!("{}", node_id).parse().unwrap());
                    (StatusCode::OK, h, axum::body::Bytes::from(content)).into_response()
                }
                Ok(Err(e)) => (
                    StatusCode::BAD_REQUEST,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error":"Failed to extract: {}"}}"#, e),
                ).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    "Task execution failed".to_string(),
                ).into_response(),
            };
        } else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
            ).into_response();
        }
    }

    // Leader: forward to selected node including self
    let (target_id, target_addr) = match get_random_node(&state).await { Some(v) => v, None => (state.node_id, state.self_http_addr.clone()) };
    if target_id == state.node_id {
        let node_id = state.node_id;
        let bytes = body.to_vec();
        let res = tokio::task::spawn_blocking(move || extract_secret_from_stego(&bytes)).await;
        return match res {
            Ok(Ok((content, mime))) => {
                let mut h = HeaderMap::new();
                h.insert("Content-Type", mime.parse().unwrap_or("application/octet-stream".parse().unwrap()));
                h.insert("X-Processed-By-Node", format!("{}", node_id).parse().unwrap());
                (StatusCode::OK, h, axum::body::Bytes::from(content)).into_response()
            }
            Ok(Err(e)) => (
                StatusCode::BAD_REQUEST,
                [("Content-Type", "application/json")],
                format!(r#"{{"error":"Failed to extract: {}"}}"#, e),
            ).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                "Task execution failed".to_string(),
            ).into_response(),
        };
    }

    // Forward to follower
    match forward_request_to_node(target_id, &target_addr, "/image/extract", &body, Some("image/png")).await {
        Ok(resp) => {
            let processed_by = resp.headers().get("x-processed-by-node").and_then(|h| h.to_str().ok()).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", target_id));
            match resp.bytes().await {
                Ok(bytes) => {
                    // Mark node healthy
                    let mut hmap = state.healthy_nodes.write().await;
                    hmap.insert(target_id, true);
                    drop(hmap);
                    let mut h = HeaderMap::new();
                    // Preserve content type from follower if present
                    if let Some(ct) = resp.headers().get("content-type").and_then(|v| v.to_str().ok()) {
                        h.insert("Content-Type", ct.parse().unwrap_or("application/octet-stream".parse().unwrap()));
                    } else {
                        h.insert("Content-Type", "application/octet-stream".parse().unwrap());
                    }
                    h.insert("X-Processed-By-Node", processed_by.parse().unwrap());
                    (StatusCode::OK, h, bytes).into_response()
                }
                Err(_e) => {
                    // Fallback: mark unhealthy and process locally
                    let mut hmap = state.healthy_nodes.write().await;
                    hmap.insert(target_id, false);
                    drop(hmap);
                    let node_id = state.node_id;
                    let bytes_local = body.to_vec();
                    let res = tokio::task::spawn_blocking(move || extract_secret_from_stego(&bytes_local)).await;
                    return match res {
                        Ok(Ok((content, mime))) => {
                            let mut h = HeaderMap::new();
                            h.insert("Content-Type", mime.parse().unwrap_or("application/octet-stream".parse().unwrap()));
                            h.insert("X-Processed-By-Node", format!("{}", node_id).parse().unwrap());
                            (StatusCode::OK, h, axum::body::Bytes::from(content)).into_response()
                        }
                        Ok(Err(e)) => (
                            StatusCode::BAD_REQUEST,
                            [("Content-Type", "application/json")],
                            format!(r#"{{"error":"Failed to extract: {}"}}"#, e),
                        ).into_response(),
                        Err(_) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            [("Content-Type", "application/json")],
                            "Task execution failed".to_string(),
                        ).into_response(),
                    };
                }
            }
        }
        Err(_e) => {
            // Fallback: mark unhealthy and process locally
            let mut hmap = state.healthy_nodes.write().await;
            hmap.insert(target_id, false);
            drop(hmap);
            let node_id = state.node_id;
            let bytes_local = body.to_vec();
            let res = tokio::task::spawn_blocking(move || extract_secret_from_stego(&bytes_local)).await;
            match res {
                Ok(Ok((content, mime))) => {
                    let mut h = HeaderMap::new();
                    h.insert("Content-Type", mime.parse().unwrap_or("application/octet-stream".parse().unwrap()));
                    h.insert("X-Processed-By-Node", format!("{}", node_id).parse().unwrap());
                    (StatusCode::OK, h, axum::body::Bytes::from(content)).into_response()
                }
                Ok(Err(e)) => (
                    StatusCode::BAD_REQUEST,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error":"Failed to extract: {}"}}"#, e),
                ).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    "Task execution failed".to_string(),
                ).into_response(),
            }
        }
    }
}

/// Embed secret image bytes into a cover image using steganography
/// Returns PNG bytes of the stego image
// Replaced legacy LSB-based approach with PNG ancillary chunk approach above

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
