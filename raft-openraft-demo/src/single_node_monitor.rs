use crate::api::{AppState, NodeId};
use openraft::ServerState;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, Instant};

/// Monitor for single-node scenario and auto-promote to leader
pub async fn monitor_single_node(state: Arc<AppState>) {
    println!("🔍 Starting single-node monitor...");
    
    let mut alone_since: Option<Instant> = None;
    let promotion_delay = Duration::from_secs(3);
    let check_interval = Duration::from_millis(500);
    
    loop {
        sleep(check_interval).await;
        
        // Get current cluster state
        let metrics = state.raft.metrics().borrow().clone();
        let current_state = metrics.state;
        let membership = metrics.membership_config.membership();
        
        // Check if we're already a leader or if cluster is initialized
        if matches!(current_state, ServerState::Leader) {
            alone_since = None;
            continue;
        }
        
        // Check if cluster is already initialized (has voters)
        let voters: Vec<NodeId> = membership.voter_ids().collect();
        if !voters.is_empty() {
            // Cluster is initialized, check if we're stuck as Candidate with all others down
            if matches!(current_state, ServerState::Candidate) {
                // Check if all other voters are unhealthy
                let http_addrs = state.http_addresses.read().await;
                let healthy = state.healthy_nodes.read().await;
                let other_voters_unhealthy = voters.iter()
                    .filter(|&id| *id != state.node_id)
                    .all(|id| !healthy.get(id).copied().unwrap_or(false));
                let voters_count = voters.len();
                drop(http_addrs);
                drop(healthy);
                
                // If we're the only voter left or all others are unhealthy, try to change membership
                if voters_count == 1 || (voters_count > 1 && other_voters_unhealthy) {
                    let node_id = state.node_id;
                    let state_clone = state.clone();
                    
                    // Start timer if not already started
                    if alone_since.is_none() {
                        println!("⏱️  Node {} (Candidate) detected all other voters are down, starting timer...", node_id);
                        alone_since = Some(Instant::now());
                    } else {
                        let elapsed = Instant::now().duration_since(alone_since.unwrap());
                        if elapsed >= promotion_delay {
                            println!("🎯 Node {} (Candidate) has been alone for {:?}, attempting to fix membership...", node_id, elapsed);
                            
                            // Try change_membership (may fail if not leader, but worth trying)
                            let result = state_clone.raft.change_membership(vec![node_id], true).await;
                            if result.is_ok() {
                                println!("✅ Node {} successfully changed membership to [{}] and became leader", node_id, node_id);
                                alone_since = None;
                            } else {
                                // If change_membership fails, we're stuck but degraded mode will handle requests
                                println!("⚠️  Node {} couldn't change membership (not leader), but will serve requests in degraded mode", node_id);
                                alone_since = None;
                            }
                        }
                    }
                } else {
                    alone_since = None;
                }
            } else {
                // Reset alone timer
                alone_since = None;
            }
            continue;
        }
        
        // Check how many nodes we know about (via HTTP addresses)
        let http_addrs = state.http_addresses.read().await;
        let known_nodes_count = http_addrs.len();
        drop(http_addrs);
        
        // We're alone if:
        // 1. No voters in membership
        // 2. No other known nodes (or only self)
        let is_alone = voters.is_empty() && known_nodes_count <= 1;
        
        if is_alone {
            // Start or continue tracking how long we've been alone
            let now = Instant::now();
            
            if alone_since.is_none() {
                println!("⏱️  Node {} detected it might be alone, starting timer...", state.node_id);
                alone_since = Some(now);
            } else {
                let elapsed = now.duration_since(alone_since.unwrap());
                
                if elapsed >= promotion_delay {
                    println!("🎯 Node {} has been alone for {:?}, auto-promoting to leader...", 
                             state.node_id, elapsed);
                    
                    // Try to initialize cluster with just ourselves
                    match auto_initialize_single_node(&state).await {
                        Ok(()) => {
                            println!("✅ Node {} successfully auto-initialized as single-node cluster", 
                                     state.node_id);
                            alone_since = None;
                        }
                        Err(e) => {
                            println!("⚠️  Failed to auto-initialize node {}: {}", state.node_id, e);
                            // Reset timer to try again later
                            alone_since = None;
                        }
                    }
                }
            }
        } else {
            // Not alone, reset timer
            if alone_since.is_some() {
                println!("👥 Node {} is no longer alone, canceling auto-promotion", state.node_id);
                alone_since = None;
            }
        }
    }
}

/// Initialize this node as a single-node cluster
async fn auto_initialize_single_node(state: &AppState) -> Result<(), String> {
    use std::collections::BTreeMap;
    
    // Create single-node membership
    let mut nodes = BTreeMap::new();
    nodes.insert(state.node_id, ());
    
    // Initialize the cluster
    state.raft.initialize(nodes).await
        .map_err(|e| format!("Failed to initialize: {}", e))?;
    
    Ok(())
}

/// Monitor for new nodes joining and trigger re-election if needed
pub async fn monitor_new_nodes(state: Arc<AppState>) {
    println!("🔍 Starting new-node monitor...");
    
    let check_interval = Duration::from_secs(5);
    let mut last_known_count = 0;
    
    loop {
        sleep(check_interval).await;
        
        // Check if we're the leader
        let metrics = state.raft.metrics().borrow().clone();
        let is_leader = matches!(metrics.state, ServerState::Leader);
        
        if !is_leader {
            continue;
        }
        
        // Get current node count
        let http_addrs = state.http_addresses.read().await;
        let current_count = http_addrs.len();
        drop(http_addrs);
        
        // If we have new nodes (count increased), we might want to add them
        if current_count > last_known_count && current_count > 1 {
            println!("📊 Node {} (leader) detected {} nodes (was {})", 
                     state.node_id, current_count, last_known_count);
            println!("💡 New nodes detected. Use /cluster/add-learner and /cluster/change-membership to add them.");
        }
        
        last_known_count = current_count;
    }
}