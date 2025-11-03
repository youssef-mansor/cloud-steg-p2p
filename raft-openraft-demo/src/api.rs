use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{Engine as _, engine::general_purpose};
use chacha20poly1305::{aead::Aead, aead::KeyInit, ChaCha20Poly1305, Key, Nonce};
use hex;
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


// Add this struct to track cluster state - for 1 node corner case
#[derive(Debug, Clone)]
pub struct ClusterState {
    pub is_single_node_mode: bool,
    pub total_nodes: usize,
}

impl Default for ClusterState {
    fn default() -> Self {
        Self {
            is_single_node_mode: false,
            total_nodes: 0,
        }
    }
}
/////////////////////////////////////////////////
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

/// Track requests completed by each node
#[derive(Debug, Clone, Default)]
pub struct ThroughputStats {
    pub completed_requests: u64,  // Requests completed in current period
    pub throughput_req_per_sec: f64,  // Calculated throughput
}

/// Application state shared across HTTP handlers - Update AppState to include cluster state to handel 1-node mode
#[derive(Clone)]
pub struct AppState {
    pub raft: Arc<RaftNode>,
    pub node_id: NodeId,
    pub http_addresses: Arc<RwLock<BTreeMap<NodeId, String>>>,
    pub self_http_addr: String,
    pub healthy_nodes: Arc<RwLock<BTreeMap<NodeId, bool>>>, // Track which nodes are healthy
    pub node_latencies: Arc<RwLock<BTreeMap<NodeId, LatencyStats>>>, // Track latency per node (kept for logging)
    pub node_throughput: Arc<RwLock<BTreeMap<NodeId, ThroughputStats>>>, // Track throughput per node
    pub cluster_state: Arc<RwLock<ClusterState>>, // Track cluster state for single-node mode
}

// Add this function to check if we should be in single-node mode
async fn check_single_node_mode(state: &AppState) -> bool {
    let http_addrs = state.http_addresses.read().await;
    let healthy_nodes = state.healthy_nodes.read().await;
    
    // Count healthy nodes (including self)
    let mut healthy_count = 0;
    for (node_id, _) in http_addrs.iter() {
        if *node_id == state.node_id {
            healthy_count += 1; // Self is always considered healthy
        } else if healthy_nodes.get(node_id).copied().unwrap_or(false) {
            healthy_count += 1;
        }
    }
    
    let total_nodes = http_addrs.len();
    let is_single_node = healthy_count == 1 && total_nodes > 0;
    
    // Update cluster state
    let mut cluster_state = state.cluster_state.write().await;
    cluster_state.is_single_node_mode = is_single_node;
    cluster_state.total_nodes = total_nodes;
    
    println!("🔍 Cluster state: {} healthy nodes out of {} total - single node mode: {}", 
             healthy_count, total_nodes, is_single_node);
    
    is_single_node
}

/// Helper function to encode bytes as base64
fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;
    
    while i < data.len() {
        let b1 = data[i];
        let b2 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b3 = if i + 2 < data.len() { data[i + 2] } else { 0 };
        
        let n = ((b1 as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);
        
        result.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
        
        if i + 1 < data.len() {
            result.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        
        if i + 2 < data.len() {
            result.push(CHARSET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        
        i += 3;
    }
    
    result
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
        .route("/image/decrypt", post(decrypt_image)) // Decrypt: extract from stego image
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

async fn forward_request_to_node_with_query(
    node_id: NodeId,
    http_addr: &str,
    path: &str,
    query_params: &str,
    body: &[u8],
) -> Result<reqwest::Response, String> {
    let url = format!("http://{}{}?{}", http_addr, path, query_params);
    // Create client with longer timeout for potentially large files
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
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
// Modify the get_random_node function to handle single-node mode
async fn get_random_node(state: &AppState) -> Option<(NodeId, String)> {
    let http_addrs = state.http_addresses.read().await;
    let healthy = state.healthy_nodes.read().await;
    
    println!("🔍 get_random_node CALLED: http_addrs has {} nodes, healthy has {} nodes", 
             http_addrs.len(), healthy.len());
    
    // Check if we're in single-node mode
    let cluster_state = state.cluster_state.read().await;
    if cluster_state.is_single_node_mode {
        println!("🎯 SINGLE-NODE MODE: Always selecting self (Node {})", state.node_id);
        return Some((state.node_id, state.self_http_addr.clone()));
    }
    drop(cluster_state);
    
    if http_addrs.is_empty() {
        println!("⚠️  CRITICAL: http_addrs is EMPTY! Falling back to self (Node {})", state.node_id);
        return Some((state.node_id, state.self_http_addr.clone()));
    }
    
    // ... rest of the existing function remains the same ...
    // Separate healthy and unhealthy nodes
    let mut healthy_node_ids: Vec<_> = Vec::new();
    let mut unhealthy_node_ids: Vec<_> = Vec::new();
    
    for (id, _) in http_addrs.iter() {
        if *id == state.node_id {
            // Self is always considered healthy
            healthy_node_ids.push(*id);
        } else if healthy.get(id).copied().unwrap_or(false) {
            healthy_node_ids.push(*id);
        } else {
            unhealthy_node_ids.push(*id);
        }
    }
    
    println!("📊 Load balance analysis: {} healthy nodes (including self), {} unhealthy", 
             healthy_node_ids.len(), unhealthy_node_ids.len());
    
    // STRATEGY: Fair distribution with slight preference for better-performing nodes
    // All nodes get equal base chance, then boost is proportional to throughput
    if healthy_node_ids.len() > 1 {
        let throughput = state.node_throughput.read().await;
        
        // Calculate weighted selection: all nodes start with equal weight (fairness)
        // then get bonus based on relative performance
        let mut weighted_nodes: Vec<(NodeId, f64)> = Vec::new();
        let mut total_weight: f64 = 0.0;
        
        // All healthy nodes start with equal base weight (for fairness)
        // This ensures even idle/new nodes get work
        let base_weight = 100.0;  // Equal base for all nodes
        
        // Calculate max throughput for performance bonus scaling
        let mut max_throughput = 0.0;
        for node_id in &healthy_node_ids {
            let tp = throughput
                .get(node_id)
                .map(|s| s.throughput_req_per_sec)
                .unwrap_or(0.0);
            if tp > max_throughput {
                max_throughput = tp;
            }
        }
        
        for node_id in &healthy_node_ids {
            let throughput_req_per_sec = throughput
                .get(node_id)
                .map(|s| s.throughput_req_per_sec)
                .unwrap_or(0.0);
            
            // Weight = base weight + small bonus (max 25% extra) for better performers
            // This prevents any node from dominating while still rewarding good performance
            let performance_bonus = if max_throughput > 0.0 {
                (throughput_req_per_sec / max_throughput) * 25.0  // Bonus: 0-25% of base
            } else {
                0.0  // No bonus if no throughput data yet
            };
            let weight = base_weight + performance_bonus;
            weighted_nodes.push((*node_id, weight));
            total_weight += weight;
        }
        
        // Select node based on weighted probability
        let mut rand_val = (random::<f64>()) * total_weight;
        for (node_id, weight) in weighted_nodes {
            rand_val -= weight;
            if rand_val <= 0.0 {
                let addr = if node_id == state.node_id {
                    state.self_http_addr.clone()
                } else {
                    http_addrs.get(&node_id)?.clone()
                };
                let actual_throughput = throughput
                    .get(&node_id)
                    .map(|s| s.throughput_req_per_sec)
                    .unwrap_or(0.0);
                let selection_probability = (weight / total_weight) * 100.0;
                println!("🎯 FAIR-WEIGHTED: Selected node {} (throughput: {:.1} req/s, weight: {:.1}, probability: {:.1}%)", 
                         node_id, actual_throughput, weight, selection_probability);
                return Some((node_id, addr));
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
// Modify the image processing handlers to check single-node mode
async fn echo_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    
    // Update cluster state and check for single-node mode
    let is_single_node = check_single_node_mode(&state).await;
    
    // Check if this is a forwarded request from the leader
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    
    // Check if we're the leader OR in single-node mode
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    let can_process = is_leader || is_single_node;
    
    // In single-node mode, we can process requests directly
    if !can_process && !is_forwarded {
        println!("🚫 Node {} (follower) dropping direct echo request - only leader processes direct requests", state.node_id);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
        ).into_response();
    }
    
    // If we can process (leader or single-node mode) and it's not forwarded, or if it's forwarded to follower
    if (!can_process && is_forwarded) || (can_process && !is_forwarded) {
        println!("📥 Node {} ({}) processing {} echo request: {} bytes", 
                state.node_id,
                if is_single_node { "single-node" } else if is_leader { "leader" } else { "follower" },
                if is_forwarded { "forwarded" } else { "direct" },
                image_size);
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
    
    // Leader in multi-node mode: randomly assign to a node (including self)
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
/// Steganography embed response with encryption key
#[derive(Debug, Serialize)]
pub struct SteganographyResponse {
    pub key: String,  // Encryption key in hex format
    pub image: String,  // Stego image in base64 format
}

// Similarly modify steg_image and decrypt_image functions:

async fn steg_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let request_start = std::time::Instant::now();
    let image_size = body.len();
    
    // Update cluster state and check for single-node mode
    let is_single_node = check_single_node_mode(&state).await;
    
    // Check Raft state first
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    let can_process = is_leader || is_single_node;
    
    println!("📨 steg_image: Node {} (is_leader={}, single_node_mode={}, is_forwarded={}) received {} bytes", 
             state.node_id, is_leader, is_single_node, is_forwarded, image_size);
    
    // Check if image is too large (max 10MB for safety)
    if image_size > 10_485_760 {
        return (
            StatusCode::BAD_REQUEST,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Image too large (max 10MB), received {} bytes"}}"#, image_size),
        ).into_response();
    }
    
    // In single-node mode, we can process requests directly
    if !can_process && !is_forwarded {
        println!("🚫 Node {} (follower) dropping direct steg request - only leader processes direct requests", state.node_id);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
        ).into_response();
    }
    
    // **FIX: Leader must call load balancing FIRST for direct requests**
    // Only non-leaders/single-node can process directly without load balancing
    if is_leader && !is_forwarded {
        println!("👉 Leader {} calling get_random_node() for load balancing...", state.node_id);
        let (target_id, target_addr) = match get_random_node(&state).await {
            Some(addr) => addr,
            None => {
                println!("⚠️  No available nodes for forwarding, processing locally");
                (state.node_id, state.self_http_addr.clone())
            }
        };
        
        println!("🎯 Leader {} selected target node {} at addr {}", state.node_id, target_id, target_addr);
        
        if target_id == state.node_id {
            // Will be processed below in the general processing block
            println!("📥 Leader {} selected SELF by load balancing - will process locally", state.node_id);
        } else {
            // Forward to another node
            println!("📤 Leader {} forwarding steg request to node {} at {}", state.node_id, target_id, target_addr);
            let forward_start = std::time::Instant::now();
            match forward_request_to_node(target_id, &target_addr, "/image/steg", &body).await {
                Ok(response) => {
                    let elapsed_ms = forward_start.elapsed().as_millis() as u64;
                    
                    // Update latency statistics
                    let mut latencies = state.node_latencies.write().await;
                    let stats = latencies.entry(target_id).or_insert(LatencyStats {
                        total_ms: 0,
                        count: 0,
                    });
                    stats.total_ms += elapsed_ms;
                    stats.count += 1;
                    drop(latencies);
                    
                    // Update throughput
                    let mut throughput = state.node_throughput.write().await;
                    let tp = throughput.entry(target_id).or_insert(ThroughputStats {
                        completed_requests: 0,
                        throughput_req_per_sec: 0.0,
                    });
                    tp.completed_requests += 1;
                    drop(throughput);
                    
                    println!("✅ Forwarded to node {} succeeded ({}ms)", target_id, elapsed_ms);
                    // Convert reqwest::Response to axum::Response
                    let status = response.status();
                    let headers = response.headers().clone();
                    match response.bytes().await {
                        Ok(body_bytes) => {
                            let mut axum_response = axum::response::Response::new(axum::body::Body::from(body_bytes.to_vec()));
                            *axum_response.status_mut() = axum::http::StatusCode::from_u16(status.as_u16()).unwrap_or(axum::http::StatusCode::OK);
                            for (k, v) in headers.iter() {
                                if let Ok(header_name) = axum::http::HeaderName::from_bytes(k.as_str().as_bytes()) {
                                    if let Ok(header_value) = axum::http::HeaderValue::from_bytes(v.as_bytes()) {
                                        axum_response.headers_mut().insert(header_name, header_value);
                                    }
                                }
                            }
                            return axum_response.into_response();
                        }
                        Err(e) => {
                            println!("❌ Failed to read response body: {}", e);
                            return (
                                StatusCode::BAD_GATEWAY,
                                [("Content-Type", "application/json")],
                                format!(r#"{{"error": "Failed to read response from node {}"}}"#, target_id),
                            ).into_response();
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Forwarding to node {} failed: {}, falling back to local processing", target_id, e);
                    // Mark node as unhealthy
                    let mut healthy = state.healthy_nodes.write().await;
                    healthy.insert(target_id, false);
                    drop(healthy);
                    // Fall through to process locally as fallback
                }
            }
        }
    }
    
    // Process locally if:
    // - Leader that selected itself OR failed to forward to another node
    // - Single-node mode with direct request  
    // - Follower receiving forwarded request
    if (is_leader && !is_forwarded) || (is_single_node && !is_forwarded) || (!can_process && is_forwarded) {
        // Generate random 32-byte key and 12-byte nonce for encryption
        let mut key_bytes = [0u8; 32];
        let mut nonce_bytes = [0u8; 12];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut key_bytes);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        
        let key = key_bytes;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Encrypt the image bytes
        let secret_bytes = &body[..];
        let ciphertext = match cipher.encrypt(nonce, secret_bytes) {
            Ok(ct) => ct,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Encryption failed: {}"}}"#, e),
                ).into_response();
            }
        };

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
        return (
            StatusCode::BAD_REQUEST,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Payload too large for cover"}}"#),
        ).into_response();
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
    let stego_img = match image::RgbaImage::from_raw(width, height, raw_bytes) {
        Some(img) => img,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Failed to rebuild image from raw bytes"}}"#),
            ).into_response();
        }
    };

    // Encode PNG to bytes in memory
    let mut png_bytes = Vec::new();
    if let Err(e) = image::DynamicImage::ImageRgba8(stego_img)
        .write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageOutputFormat::Png) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Failed to encode PNG: {}"}}"#, e),
        ).into_response();
    }
    
    // Return stego image and key in hex format
    let key_hex = hex::encode(&key);
    let image_base64 = general_purpose::STANDARD.encode(&png_bytes);
    
    let response = SteganographyResponse {
        key: key_hex,
        image: image_base64,
    };
    
    return (
        StatusCode::OK,
        [("X-Processed-By-Node", format!("{}", state.node_id).as_str())],
        axum::Json(response)
    ).into_response();
    } else {
        // Should not reach here, but handle for safety
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Invalid processing state"}}"#),
        ).into_response();
    }
}

/// Decrypt image - extract the original image from stego image
#[derive(Debug, Deserialize)]
pub struct DecryptQuery {
    pub key: String,
}

async fn decrypt_image(
    State(state): State<AppState>,
    Query(params): Query<DecryptQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    
    // Check Raft state first
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    
    println!("📨 decrypt_image: Node {} (is_leader={}, is_forwarded={}) received {} bytes", 
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
        println!("🚫 Node {} (follower) dropping direct decrypt request - only leader processes direct requests", state.node_id);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Content-Type", "application/json")],
            format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
        ).into_response();
    }
    
    // **FIX: Leader must call load balancing FIRST for direct decrypt requests**
    if is_leader && !is_forwarded {
        println!("👉 Leader {} calling get_random_node() for load balancing decrypt...", state.node_id);
        let (target_id, target_addr) = match get_random_node(&state).await {
            Some(addr) => addr,
            None => {
                println!("⚠️  No available nodes for forwarding, processing locally");
                (state.node_id, state.self_http_addr.clone())
            }
        };
        
        println!("🎯 Leader {} selected target node {} at addr {}", state.node_id, target_id, target_addr);
        
        if target_id == state.node_id {
            println!("📥 Leader {} selected SELF by load balancing - will process locally", state.node_id);
        } else {
            // Forward to another node
            println!("📤 Leader {} forwarding decrypt request to node {} at {}", state.node_id, target_id, target_addr);
            let forward_url = format!("/image/decrypt?key={}", urlencoding::encode(&params.key));
            match forward_request_to_node(target_id, &target_addr, &forward_url, &body).await {
                Ok(response) => {
                    println!("✅ Forwarded decrypt to node {} succeeded", target_id);
                    // Convert reqwest::Response to axum::Response
                    let status = response.status();
                    let headers = response.headers().clone();
                    match response.bytes().await {
                        Ok(body_bytes) => {
                            let mut axum_response = axum::response::Response::new(axum::body::Body::from(body_bytes.to_vec()));
                            *axum_response.status_mut() = axum::http::StatusCode::from_u16(status.as_u16()).unwrap_or(axum::http::StatusCode::OK);
                            for (k, v) in headers.iter() {
                                if let Ok(header_name) = axum::http::HeaderName::from_bytes(k.as_str().as_bytes()) {
                                    if let Ok(header_value) = axum::http::HeaderValue::from_bytes(v.as_bytes()) {
                                        axum_response.headers_mut().insert(header_name, header_value);
                                    }
                                }
                            }
                            return axum_response.into_response();
                        }
                        Err(e) => {
                            println!("❌ Failed to read response body: {}", e);
                            return (
                                StatusCode::BAD_GATEWAY,
                                [("Content-Type", "application/json")],
                                format!(r#"{{"error": "Failed to read response from node {}"}}"#, target_id),
                            ).into_response();
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Forwarding decrypt to node {} failed: {}, falling back to local processing", target_id, e);
                    // Mark node as unhealthy and fall through
                    let mut healthy = state.healthy_nodes.write().await;
                    healthy.insert(target_id, false);
                    drop(healthy);
                }
            }
        }
    }
    
    // If forwarded request and we're a follower, process it
    if !is_leader && is_forwarded {
        println!("📥 Node {} (follower) processing forwarded decrypt request: {} bytes", state.node_id, image_size);
        println!("🔓 Starting steganography extraction...");
        
        // Decode hex key to bytes
        let key_bytes = match hex::decode(&params.key) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("❌ Node {} invalid hex key: {}", state.node_id, e);
                return (
                    StatusCode::BAD_REQUEST,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Invalid encryption key format: {}"}}"#, e),
                ).into_response();
            }
        };
        
        // Spawn CPU-intensive image processing in blocking task to avoid blocking async runtime
        let body_clone = body.clone();
        let node_id = state.node_id;
        match tokio::task::spawn_blocking(move || extract_image_from_stego(&body_clone[..], &key_bytes)).await {
            Ok(Ok(extracted_bytes)) => {
                println!("✅ Node {} extracted image: {} bytes", node_id, extracted_bytes.len());
                return (
                    StatusCode::OK,
                    [
                        ("Content-Type", "application/octet-stream"),
                        ("X-Processed-By-Node", &format!("{}", node_id)),
                    ],
                    axum::body::Bytes::from(extracted_bytes),
                ).into_response();
            }
            Ok(Err(e)) => {
                eprintln!("❌ Node {} failed to extract image: {}", node_id, e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to extract image: {}"}}"#, e),
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
        println!("📥 Node {} (leader) processing decrypt request locally: {} bytes", state.node_id, image_size);
        println!("🔓 Starting steganography extraction...");
        
        // Decode hex key to bytes
        let key_bytes = match hex::decode(&params.key) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("❌ Node {} invalid hex key: {}", state.node_id, e);
                return (
                    StatusCode::BAD_REQUEST,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Invalid encryption key format: {}"}}"#, e),
                ).into_response();
            }
        };
        
        // Spawn CPU-intensive image processing in blocking task to avoid blocking async runtime
        let body_clone = body.clone();
        let node_id = state.node_id;
        match tokio::task::spawn_blocking(move || extract_image_from_stego(&body_clone[..], &key_bytes)).await {
            Ok(Ok(extracted_bytes)) => {
                println!("✅ Node {} extracted image: {} bytes", node_id, extracted_bytes.len());
                (
                    StatusCode::OK,
                    [
                        ("Content-Type", "application/octet-stream"),
                        ("X-Processed-By-Node", &format!("{}", node_id)),
                    ],
                    axum::body::Bytes::from(extracted_bytes),
                ).into_response()
            }
            Ok(Err(e)) => {
                eprintln!("❌ Node {} failed to extract image: {}", node_id, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Failed to extract image: {}"}}"#, e),
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
        // Forward to target node
        println!("🔄 Leader {} forwarding decrypt request to node {}", state.node_id, target_id);
        let query_string = format!("key={}", urlencoding::encode(&params.key));
        match forward_request_to_node_with_query(target_id, &target_addr, "/image/decrypt", &query_string, &body).await {
            Ok(response) => {
                match response.bytes().await {
                    Ok(response_bytes) => {
                        println!("✅ Leader {} received response from node {}: {} bytes", state.node_id, target_id, response_bytes.len());
                        (
                            StatusCode::OK,
                            [
                                ("Content-Type", "application/octet-stream"),
                                ("X-Processed-By-Node", &format!("{}", target_id)),
                            ],
                            axum::body::Bytes::from(response_bytes),
                        ).into_response()
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to read response body from node {}: {}", target_id, e);
                        (
                            StatusCode::BAD_GATEWAY,
                            [("Content-Type", "application/json")],
                            format!(r#"{{"error": "Failed to read response: {}"}}"#, e),
                        ).into_response()
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Forward to node {} failed: {}", target_id, e);
                (
                    StatusCode::BAD_GATEWAY,
                    [("Content-Type", "application/json")],
                    format!(r#"{{"error": "Forwarding failed: {}"}}"#, e),
                ).into_response()
            }
        }
    }
}

/// Extract image from stego image (LSB steganography) with user-provided key
fn extract_image_from_stego(stego_bytes: &[u8], key_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Load stego image
    let stego_img = image::load_from_memory(stego_bytes)?;
    let stego_rgba = stego_img.to_rgba8();
    let raw_bytes = stego_rgba.as_raw();
    
    // Extract bits from LSB of R, G, B channels
    let mut extracted_bits = Vec::new();
    for pixel_chunk in raw_bytes.chunks_exact(4) {
        // Extract from R channel
        extracted_bits.push(((pixel_chunk[0] & 1) != 0) as u8);
        
        // Extract from G channel
        extracted_bits.push(((pixel_chunk[1] & 1) != 0) as u8);
        
        // Extract from B channel
        extracted_bits.push(((pixel_chunk[2] & 1) != 0) as u8);
    }
    
    // Convert bits to bytes (LSB-first per byte)
    let mut payload_bytes = Vec::new();
    for byte_bits in extracted_bits.chunks_exact(8) {
        let mut byte = 0u8;
        for (i, &bit) in byte_bits.iter().enumerate() {
            byte |= bit << i;
        }
        payload_bytes.push(byte);
    }
    
    // Parse payload: [4 bytes len][12 bytes nonce][ciphertext]
    if payload_bytes.len() < 16 {
        return Err("Payload too small".into());
    }
    
    let ct_len = u32::from_le_bytes([
        payload_bytes[0],
        payload_bytes[1],
        payload_bytes[2],
        payload_bytes[3],
    ]) as usize;
    
    let nonce_bytes = &payload_bytes[4..16];
    let ciphertext = &payload_bytes[16..16 + ct_len];
    
    if payload_bytes.len() < 16 + ct_len {
        return Err("Incomplete ciphertext".into());
    }
    
    // Validate key length (must be exactly 32 bytes for ChaCha20Poly1305)
    if key_bytes.len() != 32 {
        return Err(format!("Invalid key size: expected 32 bytes, got {}", key_bytes.len()).into());
    }
    
    // Decrypt with the user-provided key
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed - invalid key or corrupted data: {}", e))?;
    
    Ok(plaintext)
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
