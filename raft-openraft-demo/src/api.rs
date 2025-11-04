use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    body::Bytes,
    Json, Router,
};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::cors::{CorsLayer, Any};
use openraft::{Raft, ServerState};
use openraft_memstore::TypeConfig;
use rand::random;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration, Instant};
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
    pub degraded_ok: Arc<AtomicBool>, // Allow degraded mode (stateless requests) when true
    pub node_throughput_1: Arc<AtomicU64>, // Atomic counter for node 1 requests
    pub node_throughput_2: Arc<AtomicU64>, // Atomic counter for node 2 requests
    pub node_throughput_3: Arc<AtomicU64>, // Atomic counter for node 3 requests
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
    
    // Start peer health pings
    start_peer_health_pings(state.clone());
    
    // Start no-leader monitor (allows degraded mode after timeout)
    start_no_leader_monitor(state.clone());
    
    // Start leader watchdog (proactively shrinks membership before losing quorum)
    start_leader_watchdog(state.clone());
}

/// Periodic peer health pings to keep healthy_nodes updated
pub fn start_peer_health_pings(state: AppState) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        loop {
            sleep(Duration::from_secs(1)).await;
            
            let addrs = state.http_addresses.read().await.clone();
            for (id, addr) in addrs {
                if id == state.node_id {
                    continue;
                }
                
                let url = format!("http://{}/metrics", addr);
                let ok = match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => true,
                    _ => false,
                };
                
                let mut h = state.healthy_nodes.write().await;
                h.insert(id, ok);
                drop(h);
            }
        }
    });
}

/// Monitor for no-leader condition and enable degraded mode after timeout
pub fn start_no_leader_monitor(state: AppState) {
    tokio::spawn(async move {
        let mut no_leader_since: Option<Instant> = None;
        let threshold = Duration::from_secs(3);
        
        loop {
            sleep(Duration::from_millis(300)).await;
            
            let m = state.raft.metrics().borrow().clone();
            let has_leader = m.current_leader.is_some();
            
            if has_leader {
                no_leader_since = None;
                state.degraded_ok.store(false, Ordering::Relaxed);
                continue;
            }
            
            if no_leader_since.is_none() {
                no_leader_since = Some(Instant::now());
            }
            
            if let Some(start) = no_leader_since {
                if start.elapsed() >= threshold {
                    // Check if all peers appear unhealthy
                    let (others_unhealthy, count) = {
                        let http_addrs = state.http_addresses.read().await;
                        let healthy = state.healthy_nodes.read().await;
                        let count = http_addrs.len();
                        let others_unhealthy = http_addrs.iter()
                            .filter(|(id, _)| **id != state.node_id)
                            .all(|(id, _)| !healthy.get(id).copied().unwrap_or(false));
                        (others_unhealthy, count)
                    };
                    
                    if others_unhealthy || count <= 1 {
                        state.degraded_ok.store(true, Ordering::Relaxed);
                        println!("🆘 Node {}: No leader for {:?}, all others unhealthy - enabling degraded mode", 
                            state.node_id, start.elapsed());
                    }
                }
            }
        }
    });
}

/// Leader watchdog: proactively shrink membership before losing quorum
pub fn start_leader_watchdog(state: AppState) {
    tokio::spawn(async move {
        let check_every = Duration::from_millis(500);
        let suspect_for = Duration::from_secs(3);
        let mut bad_since: Option<Instant> = None;
        
        loop {
            sleep(check_every).await;
            
            let m = state.raft.metrics().borrow().clone();
            if !matches!(m.state, ServerState::Leader) {
                bad_since = None;
                continue;
            }
            
            // Get current voters from membership
            let voters: Vec<NodeId> = m.membership_config
                .membership()
                .voter_ids()
                .collect();
            
            // Count unhealthy voters (excluding self)
            let healthy = state.healthy_nodes.read().await;
            let unhealthy_count = voters.iter()
                .filter(|&&id| id != state.node_id)
                .filter(|&&id| !healthy.get(&id).copied().unwrap_or(false))
                .count();
            drop(healthy);
            
            // Calculate if we're losing quorum
            let majority = (voters.len() / 2) + 1;
            let max_fail_to_keep_quorum = voters.len() + 1 - majority - 1; // e.g., 3 voters -> 0 max failures
            let losing_quorum = unhealthy_count > max_fail_to_keep_quorum;
            
            if losing_quorum {
                if bad_since.is_none() {
                    bad_since = Some(Instant::now());
                    println!("⚠️ Node {} (leader) detected {} unhealthy voters, may lose quorum soon", 
                        state.node_id, unhealthy_count);
                }
                
                if let Some(start) = bad_since {
                    if start.elapsed() >= suspect_for {
                        println!("🎯 Node {} (leader) proactively shrinking membership to [{}] before losing quorum", 
                            state.node_id, state.node_id);
                        match state.raft.change_membership(vec![state.node_id], true).await {
                            Ok(_) => {
                                println!("✅ Node {} successfully changed membership to [{}]", state.node_id, state.node_id);
                                bad_since = None;
                            }
                            Err(e) => {
                                eprintln!("❌ Failed to change membership: {}", e);
                                bad_since = None; // Reset to try again later
                            }
                        }
                    }
                }
            } else {
                bad_since = None;
            }
        }
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
    // Configure CORS to allow cross-origin requests from web UI
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);  // CRITICAL: Expose custom headers like X-Processed-By-Node to browser
    
    Router::new()
        .route("/", get(root))
        .route("/metrics", get(metrics))
        .route("/metrics/throughput", get(metrics_throughput))
        .route("/cluster/init", post(init_cluster))
        .route("/cluster/add-learner", post(add_learner))
        .route("/cluster/change-membership", post(change_membership))
        .route("/image/echo", post(echo_image))      // Echo: return same image
        .route("/image/steg", post(steg_image_multipart))      // Steganography: embed and return stego image (multipart)
        .route("/image/extract", post(extract_image))          // Extract secret from stego image
        .layer(cors)
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

/// Get throughput metrics for all nodes (as tracked by this node, typically the leader)
async fn metrics_throughput(State(state): State<AppState>) -> impl IntoResponse {
    // Read atomic counters (very fast, no lock needed)
    let count_1 = state.node_throughput_1.load(Ordering::Relaxed);
    let count_2 = state.node_throughput_2.load(Ordering::Relaxed);
    let count_3 = state.node_throughput_3.load(Ordering::Relaxed);
    
    // Collect throughput for all nodes
    let mut node_throughputs = serde_json::Map::new();
    node_throughputs.insert("node_1".to_string(), serde_json::json!(count_1));
    node_throughputs.insert("node_2".to_string(), serde_json::json!(count_2));
    node_throughputs.insert("node_3".to_string(), serde_json::json!(count_3));
    
    Json(ApiResponse {
        success: true,
        data: Some(serde_json::json!({
            "reporting_node_id": state.node_id,
            "node_throughputs": node_throughputs,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0),
        })),
        error: None,
    })
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
    // Create client with shorter timeout to avoid hanging on crashed nodes
    // Use 5 seconds - enough for normal requests but fast failure for down nodes
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .connect_timeout(std::time::Duration::from_secs(2))
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
    
    // NEVER select unhealthy nodes if we have healthy alternatives
    // This prevents hanging on down nodes
    
    // Normal case: select from healthy nodes
    if healthy_node_ids.is_empty() {
        // No healthy nodes (shouldn't happen since self is always healthy), fallback to self
        println!("⚠️ No healthy nodes found, using self as fallback");
        return Some((state.node_id, state.self_http_addr.clone()));
    }
    
    // If self is the only healthy node, always select self
    if healthy_node_ids.len() == 1 && healthy_node_ids[0] == state.node_id {
        println!("✅ Only self is healthy, processing locally");
        return Some((state.node_id, state.self_http_addr.clone()));
    }
    
    // Select randomly from healthy nodes (excluding self only if there are other healthy nodes)
    let healthy_non_self: Vec<_> = healthy_node_ids.iter()
        .filter(|&&id| id != state.node_id)
        .copied()
        .collect();
    
    if !healthy_non_self.is_empty() {
        // Prefer other healthy nodes for load balancing, but include self in the pool
        let all_healthy = healthy_node_ids;
        let idx = random::<usize>() % all_healthy.len();
        let selected_id = all_healthy[idx];
        let addr = if selected_id == state.node_id {
            state.self_http_addr.clone()
        } else {
            http_addrs.get(&selected_id)?.clone()
        };
        Some((selected_id, addr))
    } else {
        // Only self is healthy
        Some((state.node_id, state.self_http_addr.clone()))
    }
}

/// Echo image - receive and return immediately (no processing)
/// Load balancing: Followers reject direct requests, Leader forwards randomly
async fn echo_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let image_size = body.len();
    
    // Check if this is a forwarded request from the leader
    let is_forwarded = headers.contains_key("x-raft-forwarded");
    
    // Check if we're the leader
    let metrics = state.raft.metrics().borrow().clone();
    let is_leader = matches!(metrics.state, ServerState::Leader);
    
    // Followers only accept forwarded requests, not direct client requests
    if !is_leader && !is_forwarded {
        // Check if degraded mode is enabled
        let can_degrade = state.degraded_ok.load(Ordering::Relaxed);
        if !can_degrade {
            println!("🚫 Node {} (follower) dropping direct echo request - only leader processes direct requests", state.node_id);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Content-Type", "application/json")],
                format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
            ).into_response();
        }
        // Degraded mode: process request
        println!("🆘 Node {} (degraded mode) processing echo request directly - degraded_ok=true", state.node_id);
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
        println!("✅ Node {} (leader) processing echo request locally: {} bytes", state.node_id, image_size);
        println!("📤 Echoing back {} bytes", image_size);
        
        // Track throughput (atomic, no lock needed)
        match state.node_id {
            1 => state.node_throughput_1.fetch_add(1, Ordering::Relaxed),
            2 => state.node_throughput_2.fetch_add(1, Ordering::Relaxed),
            3 => state.node_throughput_3.fetch_add(1, Ordering::Relaxed),
            _ => 0,
        };
        
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
                        
                        // Track throughput for the node that processed it (atomic, no lock needed)
                        match target_id {
                            1 => state.node_throughput_1.fetch_add(1, Ordering::Relaxed),
                            2 => state.node_throughput_2.fetch_add(1, Ordering::Relaxed),
                            3 => state.node_throughput_3.fetch_add(1, Ordering::Relaxed),
                            _ => 0,
                        };
                        
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
                eprintln!("❌ Failed to forward echo to node {}: {} - marking as unhealthy, processing locally as fallback", target_id, e);
                // Mark node as unhealthy when forwarding fails
                let mut healthy = state.healthy_nodes.write().await;
                healthy.insert(target_id, false);
                drop(healthy);
                
                // Fallback: process locally instead of failing
                println!("🔄 Node {} (leader) falling back to local echo processing after forward failure", state.node_id);
                
                // Track throughput (atomic, no lock needed)
                match state.node_id {
                    1 => state.node_throughput_1.fetch_add(1, Ordering::Relaxed),
                    2 => state.node_throughput_2.fetch_add(1, Ordering::Relaxed),
                    3 => state.node_throughput_3.fetch_add(1, Ordering::Relaxed),
                    _ => 0,
                };
                
                return (
                    StatusCode::OK,
                    [
                        ("Content-Type", "application/octet-stream"),
                        ("X-Processed-By-Node", &format!("{}", state.node_id)),
                    ],
                    body,
                ).into_response();
            }
        }
    }
}

/// Embed image - receive image and return stego image with secret embedded
/// Load balancing: Followers reject direct requests, Leader forwards randomly
#[axum::debug_handler]
async fn steg_image_multipart(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Note: We can't extract HeaderMap with Multipart, so we'll check forwarded status
    // via a workaround - when forwarding, we'll include it in the multipart fields or
    // check it from the request context. For now, assume not forwarded for direct requests.
    // The forwarded check will be handled by checking if the node is leader or follower.
    let is_forwarded = false; // Will be true when request comes from leader forwarding
    
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
    // Note: is_forwarded is set above - when multipart is used, we check via internal forwarding

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
            // Check if degraded mode is enabled (via no-leader monitor)
            let can_degrade = state.degraded_ok.load(Ordering::Relaxed);
            
            if can_degrade {
                println!("🆘 Node {} (degraded mode) processing steg request directly - degraded_ok=true", state.node_id);
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
            
                return (
                StatusCode::SERVICE_UNAVAILABLE,
                    [("Content-Type", "application/json")],
                format!(r#"{{"error": "Node {} is not the leader. Request dropped."}}"#, state.node_id),
                ).into_response();
        }
    }

    // Leader: forward to a randomly selected node (including self)
    let (target_id, target_addr) = match get_random_node(&state).await { 
        Some(v) => v, 
        None => {
            // No nodes available, process locally as fallback
            println!("⚠️ Node {} (leader) has no available nodes, processing locally", state.node_id);
            (state.node_id, state.self_http_addr.clone())
        }
    };
    
    if target_id == state.node_id {
        // Process locally
        println!("✅ Node {} (leader) processing steg request locally", state.node_id);
        let node_id = state.node_id;
        let sm = secret_mime.clone();
        let res = tokio::task::spawn_blocking(move || embed_cover_with_secret_chunk(&cover, &secret, sm.as_deref())).await;
        
        // Track throughput (atomic, no lock needed)
        match node_id {
            1 => state.node_throughput_1.fetch_add(1, Ordering::Relaxed),
            2 => state.node_throughput_2.fetch_add(1, Ordering::Relaxed),
            3 => state.node_throughput_3.fetch_add(1, Ordering::Relaxed),
            _ => 0,
        };
        
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
                    
                    // Track throughput for the node that processed it (atomic, no lock needed)
                    if let Ok(proc_node_id) = processed_by.parse::<NodeId>() {
                        match proc_node_id {
                            1 => state.node_throughput_1.fetch_add(1, Ordering::Relaxed),
                            2 => state.node_throughput_2.fetch_add(1, Ordering::Relaxed),
                            3 => state.node_throughput_3.fetch_add(1, Ordering::Relaxed),
                            _ => 0,
                        };
                    }
                    
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

/// Simple multipart parser - extracts cover and secret fields
fn parse_multipart_simple(body: &[u8], boundary: &str) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<String>) {
    let delim = format!("--{}", boundary);
    let delim_bytes = delim.as_bytes();
    let mut cover: Option<Vec<u8>> = None;
    let mut secret: Option<Vec<u8>> = None;
    let mut secret_mime: Option<String> = None;
    
    // Find boundary positions
    let mut pos = 0;
    while let Some(boundary_pos) = body[pos..].windows(delim_bytes.len()).position(|w| w == delim_bytes) {
        let part_start = pos + boundary_pos + delim_bytes.len();
        if part_start >= body.len() { break; }
        
        // Skip CRLF after boundary
        let mut data_start = part_start;
        if data_start + 2 <= body.len() && &body[data_start..data_start+2] == b"\r\n" {
            data_start += 2;
        }
        
        // Find end of headers (CRLFCRLF) or next boundary
        let headers_end = body[data_start..].windows(4).position(|w| w == b"\r\n\r\n");
        let headers_end = headers_end.map(|i| data_start + i + 4).unwrap_or(data_start);
        
        // Check headers for field name
        let headers = &body[data_start..headers_end.min(body.len())];
        let headers_str = String::from_utf8_lossy(headers);
        
        if headers_str.contains("name=\"cover\"") {
            // Find next boundary or end
            let next_boundary = body[headers_end..].windows(delim_bytes.len()).position(|w| w == delim_bytes);
            let data_end = next_boundary.map(|i| headers_end + i).unwrap_or(body.len());
            // Remove trailing CRLF before boundary
            let mut data_end_adj = data_end;
            if data_end_adj >= 2 && &body[data_end_adj - 2..data_end_adj] == b"\r\n" {
                data_end_adj -= 2;
            }
            cover = Some(body[headers_end..data_end_adj].to_vec());
        } else if headers_str.contains("name=\"secret\"") {
            // Extract content-type from headers
            if let Some(ct_line) = headers_str.lines().find(|l| l.to_lowercase().starts_with("content-type:")) {
                secret_mime = ct_line.split(':').nth(1).map(|s| s.trim().to_string());
            }
            // Find next boundary or end
            let next_boundary = body[headers_end..].windows(delim_bytes.len()).position(|w| w == delim_bytes);
            let data_end = next_boundary.map(|i| headers_end + i).unwrap_or(body.len());
            let mut data_end_adj = data_end;
            if data_end_adj >= 2 && &body[data_end_adj - 2..data_end_adj] == b"\r\n" {
                data_end_adj -= 2;
            }
            secret = Some(body[headers_end..data_end_adj].to_vec());
        }
        
        // Move to next boundary
        pos = part_start;
        if pos >= body.len() { break; }
    }
    
    (cover, secret, secret_mime)
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
#[axum::debug_handler]
async fn extract_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
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
            // Check if degraded mode is enabled (via no-leader monitor)
            let can_degrade = state.degraded_ok.load(Ordering::Relaxed);
            
            if can_degrade {
                println!("🆘 Node {} (degraded mode) processing extract request directly - degraded_ok=true", state.node_id);
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
            // Extract headers before consuming response with bytes()
            let processed_by = resp.headers().get("x-processed-by-node").and_then(|h| h.to_str().ok()).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", target_id));
            let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
            match resp.bytes().await {
                    Ok(bytes) => {
                    // Mark node healthy
                    let mut hmap = state.healthy_nodes.write().await;
                    hmap.insert(target_id, true);
                    drop(hmap);
                    let mut h = HeaderMap::new();
                    // Preserve content type from follower if present
                    if let Some(ct) = content_type {
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
