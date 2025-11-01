use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chacha20poly1305::{aead::Aead, aead::KeyInit, ChaCha20Poly1305, Key, Nonce};
use openraft::{Raft, RaftMetrics, ServerState};
use openraft_memstore::TypeConfig;
use rand::rngs::OsRng;
use rand::{random, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{CorsLayer, Any};

pub type NodeId = u64;
pub type RaftNode = Raft<TypeConfig>;

/// Latency statistics for a single node
#[derive(Clone, Copy, Debug)]
pub struct LatencyStats {
    pub total_ms: u64,
    pub count: u64,
}

impl LatencyStats {
    pub fn average_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_ms as f64 / self.count as f64
        }
    }
}

/// Application state shared across HTTP handlers
#[derive(Clone)]
pub struct AppState {
    pub raft: Arc<RaftNode>,
    pub node_id: NodeId,
    pub http_addresses: Arc<RwLock<BTreeMap<NodeId, String>>>,
    pub self_http_addr: String,
    pub healthy_nodes: Arc<RwLock<BTreeMap<NodeId, bool>>>, // Track which nodes are healthy
    pub node_latencies: Arc<RwLock<BTreeMap<NodeId, LatencyStats>>>, // Track latency per node
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
    // Configure CORS to allow cross-origin requests from web UI
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);  // CRITICAL: Expose custom headers like X-Processed-By-Node to browser
    
    Router::new()
        .route("/", get(root))
        .route("/metrics", get(metrics))
        .route("/cluster/init", post(init_cluster))
        .route("/cluster/add-learner", post(add_learner))
        .route("/cluster/change-membership", post(change_membership))
        .route("/image/echo", post(echo_image))      // Echo: return same image
        .route("/image/steg", post(steg_image))      // Steganography: embed and return stego image
        .layer(cors)
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
) -> Result<reqwest::Response, String> {
    let url = format!("http://{}{}", http_addr, path);
    // Create client with timeout to avoid hanging on crashed nodes
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    client
        .post(&url)
        .header("X-Raft-Forwarded", "true")
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| format!("Failed to forward to node {} at {}: {}", node_id, http_addr, e))
}

/// Get a node ID using latency-aware weighted selection
/// Prefers fast, healthy nodes over slow ones using inverse latency weighting
async fn get_random_node(state: &AppState) -> Option<(NodeId, String)> {
    let http_addrs = state.http_addresses.read().await;
    let healthy = state.healthy_nodes.read().await;
    let latencies = state.node_latencies.read().await;
    
    println!("🔍 get_random_node CALLED: http_addrs has {} nodes, healthy has {} nodes", 
             http_addrs.len(), healthy.len());
    
    if http_addrs.is_empty() {
        println!("⚠️  CRITICAL: http_addrs is EMPTY! Falling back to self (Node {})", state.node_id);
        return Some((state.node_id, state.self_http_addr.clone()));
    }
    
    // Separate healthy and unhealthy nodes, also track non-self nodes for load distribution
    let mut healthy_node_ids: Vec<_> = Vec::new();
    let mut healthy_non_self_nodes: Vec<_> = Vec::new();
    let mut unhealthy_node_ids: Vec<_> = Vec::new();
    
    for (id, _) in http_addrs.iter() {
        if *id == state.node_id {
            healthy_node_ids.push(*id);
        } else if healthy.get(id).copied().unwrap_or(false) {
            healthy_node_ids.push(*id);
            healthy_non_self_nodes.push(*id);
        } else {
            unhealthy_node_ids.push(*id);
        }
    }
    
    println!("📊 Load balance analysis: self={}, non_self_healthy={}, unhealthy={}", 
             state.node_id, healthy_non_self_nodes.len(), unhealthy_node_ids.len());
    
    // STRATEGY: Latency-aware weighted selection for better load distribution
    // Faster nodes get higher selection probability (inverse latency weighting)
    if !healthy_non_self_nodes.is_empty() {
        let forward_preference = (random::<usize>() % 100) < 80; // 80% preference to forward
        if forward_preference {
            // Calculate weighted selection based on inverse latency
            let mut weighted_nodes: Vec<(NodeId, f64)> = Vec::new();
            let mut total_weight: f64 = 0.0;
            
            for node_id in &healthy_non_self_nodes {
                let latency_ms = latencies
                    .get(node_id)
                    .map(|s| s.average_ms())
                    .unwrap_or(50.0); // Default 50ms if no data
                
                // Weight = 1 / latency, so faster nodes get higher weight
                // Add small offset to avoid division issues with very low latencies
                let weight = 1.0 / (latency_ms + 1.0);
                weighted_nodes.push((*node_id, weight));
                total_weight += weight;
            }
            
            // Select node based on weighted probability
            let mut rand_val = (random::<f64>()) * total_weight;
            for (node_id, weight) in weighted_nodes {
                rand_val -= weight;
                if rand_val <= 0.0 {
                    let addr = http_addrs.get(&node_id)?.clone();
                    let avg_latency = latencies
                        .get(&node_id)
                        .map(|s| format!("{:.1}ms", s.average_ms()))
                        .unwrap_or("N/A".to_string());
                    println!("🎯 LATENCY-WEIGHTED: Selected node {} (avg latency: {})", node_id, avg_latency);
                    return Some((node_id, addr));
                }
            }
        }
    }
    
    // If we have healthy nodes and random check (20% chance), give unhealthy nodes a chance
    let use_unhealthy = !healthy_node_ids.is_empty() && 
                        !unhealthy_node_ids.is_empty() && 
                        (random::<usize>() % 5) == 0;
    
    if use_unhealthy && !unhealthy_node_ids.is_empty() {
        let idx = random::<usize>() % unhealthy_node_ids.len();
        let selected_id = unhealthy_node_ids[idx];
        let addr = http_addrs.get(&selected_id)?.clone();
        println!("🔍 Probation: trying unhealthy node {} to check if it recovered", selected_id);
        return Some((selected_id, addr));
    }
    
    // Normal case: select from healthy nodes
    if healthy_node_ids.is_empty() {
        if unhealthy_node_ids.is_empty() {
            println!("⚠️  No nodes available, processing locally");
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
    
    println!("✅ get_random_node selected node {} from {} healthy nodes", selected_id, healthy_node_ids.len());
    
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
        match forward_request_to_node(target_id, &target_addr, "/image/echo", &body).await {
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
async fn steg_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    
    // Check Raft state first
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    
    println!("📨 steg_image: Node {} (is_leader={}, is_forwarded={}) received {} bytes", 
             state.node_id, is_leader, is_forwarded, image_size);
    
    // Check if image is too large (max 10MB for safety)
    if image_size > 10_485_760 {
        return (
            StatusCode::BAD_REQUEST,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Image too large (max 10MB), received {} bytes"}}"#, image_size),
        ).into_response();
    }
    
    // Followers only accept forwarded requests, not direct client requests
    if !is_leader && !is_forwarded {
        println!("🚫 Node {} (follower) dropping direct steg request - only leader processes direct requests", state.node_id);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
        ).into_response();
    }
    
    // If forwarded request and we're a follower, process it
    if !is_leader && is_forwarded {
        println!("📥 Node {} (follower) processing forwarded steg request: {} bytes", state.node_id, image_size);
        println!("🔐 Starting steganography embedding...");
        println!("🔒 Encrypting and embedding image...");
        // Spawn CPU-intensive image processing in blocking task to avoid blocking async runtime
        let body_clone = body.clone();
        let node_id = state.node_id;
        match tokio::task::spawn_blocking(move || embed_image_into_cover(&body_clone[..])).await {
            Ok(Ok(stego_bytes)) => {
                println!("✅ Node {} created stego image: {} bytes (original: {} bytes)", 
                         node_id, stego_bytes.len(), image_size);
                return (
                    StatusCode::OK,
                    [
                        ("Content-Type", "image/png"),
                        ("X-Processed-By-Node", &format!("{}", node_id)),
                    ],
                    axum::body::Bytes::from(stego_bytes),
                ).into_response();
            }
            Ok(Err(e)) => {
                eprintln!("❌ Node {} failed to embed image: {}", node_id, e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
                ).into_response();
            }
            Err(e) => {
                eprintln!("❌ Node {} task join error: {}", node_id, e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Task execution failed"}}"#),
                ).into_response();
            }
        }
    }
    
    // Leader: randomly assign to a node (including self)
    println!("👉 Leader {} calling get_random_node()...", state.node_id);
    let (target_id, target_addr) = match get_random_node(&state).await {
        Some(addr) => addr,
        None => {
            println!("⚠️  No available nodes for forwarding, processing locally");
            (state.node_id, state.self_http_addr.clone())
        }
    };
    
    println!("🎯 Leader {} selected target node {} at addr {}", state.node_id, target_id, target_addr);
    
    if target_id == state.node_id {
        // Process locally
        println!("📥 Node {} (leader) processing steg request locally: {} bytes", state.node_id, image_size);
        println!("🔐 Starting steganography embedding...");
        println!("🔒 Encrypting and embedding image...");
        // Spawn CPU-intensive image processing in blocking task to avoid blocking async runtime
        let body_clone = body.clone();
        let node_id = state.node_id;
        match tokio::task::spawn_blocking(move || embed_image_into_cover(&body_clone[..])).await {
            Ok(Ok(stego_bytes)) => {
                println!("✅ Node {} created stego image: {} bytes (original: {} bytes)", 
                         node_id, stego_bytes.len(), image_size);
                (
                    StatusCode::OK,
                    [
                        ("Content-Type", "image/png"),
                        ("X-Processed-By-Node", &format!("{}", node_id)),
                    ],
                    axum::body::Bytes::from(stego_bytes),
                ).into_response()
            }
            Ok(Err(e)) => {
                eprintln!("❌ Node {} failed to embed image: {}", node_id, e);
                eprintln!("   Error details: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to embed image: {}"}}"#, e),
                ).into_response()
            }
            Err(e) => {
                eprintln!("❌ Node {} task join error: {}", node_id, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Task execution failed"}}"#),
                ).into_response()
            }
        }
    } else {
        // Forward to selected node with retry logic and latency tracking
        let start_time = std::time::Instant::now();
        println!("🔄 Node {} (leader) forwarding steg request to node {} at {}", 
                state.node_id, target_id, target_addr);
        match forward_request_to_node(target_id, &target_addr, "/image/steg", &body).await {
            Ok(response) => {
                match response.bytes().await {
                    Ok(bytes) => {
                        let elapsed_ms = start_time.elapsed().as_millis() as u64;
                        println!("✅ Received stego image from node {}: {} bytes (latency: {}ms)", 
                                target_id, bytes.len(), elapsed_ms);
                        
                        // Record latency for this node
                        let mut latencies = state.node_latencies.write().await;
                        let stats = latencies.entry(target_id).or_insert(LatencyStats {
                            total_ms: 0,
                            count: 0,
                        });
                        stats.total_ms += elapsed_ms;
                        stats.count += 1;
                        drop(latencies);
                        
                        // Mark node as healthy on successful response
                        let mut healthy = state.healthy_nodes.write().await;
                        healthy.insert(target_id, true);
                        drop(healthy);
                        
                        let mut headers = HeaderMap::new();
                        headers.insert("Content-Type", "image/png".parse().unwrap());
                        headers.insert("X-Processed-By-Node", format!("{}", target_id).parse().unwrap());
                        (StatusCode::OK, headers, bytes).into_response()
                    }
                    Err(e) => {
                        let elapsed_ms = start_time.elapsed().as_millis() as u64;
                        eprintln!("❌ Failed to read response from node {}: {} (latency: {}ms)", target_id, e, elapsed_ms);
                        
                        // Record latency (slow failures matter too!)
                        let mut latencies = state.node_latencies.write().await;
                        let stats = latencies.entry(target_id).or_insert(LatencyStats {
                            total_ms: 0,
                            count: 0,
                        });
                        stats.total_ms += elapsed_ms;
                        stats.count += 1;
                        drop(latencies);
                        
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
                let elapsed_ms = start_time.elapsed().as_millis() as u64;
                eprintln!("❌ Failed to forward to node {}: {} (latency: {}ms) - marking as unhealthy", target_id, e, elapsed_ms);
                
                // Record latency even on failure
                let mut latencies = state.node_latencies.write().await;
                let stats = latencies.entry(target_id).or_insert(LatencyStats {
                    total_ms: 0,
                    count: 0,
                });
                stats.total_ms += elapsed_ms;
                stats.count += 1;
                drop(latencies);
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
