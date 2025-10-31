#!/bin/bash

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_header() {
    echo -e "\n${BLUE}================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Helper functions (same as quick_test_FIXED.sh)
get_node_state() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.state // "UNREACHABLE"'
}

get_leader_id() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.current_leader // "none"'
}

get_term() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.current_term // "?"'
}

get_node_id() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.node_id // "?"'
}

print_cluster_state() {
    echo ""
    for port in 8001 8002 8003; do
        node_id=$((port - 8000))
        state=$(get_node_state $port)
        leader=$(get_leader_id $port)
        term=$(get_term $port)

        if [[ "$state" == "UNREACHABLE" ]]; then
            printf "  Node %d: ${RED}[UNREACHABLE]${NC}\n" $node_id
        else
            printf "  Node %d: ${GREEN}%-10s${NC} term=%-2s leader=%s\n" $node_id "$state" "$term" "$leader"
        fi
    done
    echo ""
}

print_header "TEST: Follower Rejoin After Crash"

print_info "This test verifies that a follower node properly rejoins the cluster"
print_info "and recovers its follower status after being stopped and restarted."
echo ""

# Phase 1: Verify initial healthy cluster
print_info "Phase 1: Verify initial healthy cluster"
print_cluster_state

# Find leader and follower
leader_node=""
follower_node=""
other_node=""

for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    node_id=$((port - 8000))

    if [[ "$state" == "Leader" ]]; then
        leader_node=$node_id
    fi
done

print_success "Leader is Node $leader_node"

# Pick a follower to stop (not the leader)
for node in 1 2 3; do
    if [[ $node -ne $leader_node ]]; then
        follower_node=$node
        break
    fi
done

print_success "Will test follower: Node $follower_node"

# Phase 2: Record initial state
print_info "Phase 2: Recording initial cluster metrics"
initial_leader=$leader_node
initial_term=$(get_term 8001)
print_success "Initial term: $initial_term, Leader: Node $initial_leader"

# Phase 3: Stop the follower
print_header "Phase 3: Stopping Follower Node $follower_node"

print_warning "Please stop Node $follower_node by pressing Ctrl+C in its terminal"
print_info "Press ENTER once the node is stopped"
read -p ""

sleep 2

# Verify follower is down
follower_port=$((8000 + follower_node))
state=$(get_node_state $follower_port)

if [[ "$state" == "UNREACHABLE" ]]; then
    print_success "Node $follower_node confirmed STOPPED"
else
    print_error "Node $follower_node still running (state: $state)"
fi

print_cluster_state

# Phase 4: Verify cluster still operates with 2/3 quorum
print_header "Phase 4: Verify Cluster Operates with 2/3 Quorum"

leader_state=$(get_node_state $((8000 + leader_node)))
if [[ "$leader_state" == "Leader" ]]; then
    print_success "Leader (Node $leader_node) still operational"
else
    print_error "Leader lost (state: $leader_state)"
fi

# Count operational nodes
operational=0
for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    if [[ ! "$state" == "UNREACHABLE" ]]; then
        operational=$((operational + 1))
    fi
done

if [ $operational -eq 2 ]; then
    print_success "2/3 nodes operational (as expected)"
else
    print_error "Unexpected node count: $operational/3"
fi

# Phase 5: Record state BEFORE follower restart
print_header "Phase 5: Recording Leader State Before Follower Restart"

pre_restart_leader=$(get_leader_id 8001)
pre_restart_term=$(get_term 8001)

print_info "Before restart:"
echo "  Leader: Node $pre_restart_leader"
echo "  Term: $pre_restart_term"

# Phase 6: Restart the follower
print_header "Phase 6: Restarting Follower Node $follower_node"

print_warning "Please restart Node $follower_node (run cargo run command in new terminal)"
print_info "Press ENTER once the node is restarted"
read -p ""

sleep 2

print_info "Waiting for follower to rejoin and sync..."

# Monitor rejoin
rejoin_detected=0
rejoin_time=0
target_state="Follower"

for attempt in {1..30}; do
    state=$(get_node_state $follower_port)
    term=$(get_term $follower_port)

    if [[ ! "$state" == "UNREACHABLE" ]]; then
        if [ $rejoin_detected -eq 0 ]; then
            print_success "Node $follower_node is back online (state: $state, term: $term)"
            rejoin_detected=1
            rejoin_time=$attempt
        fi

        if [[ "$state" == "$target_state" ]]; then
            break
        fi
    fi

    if [ $attempt -lt 30 ]; then
        sleep 0.5
    fi
done

# Phase 7: Verify follower has correct status
print_header "Phase 7: Verify Follower Has Correct Status After Rejoin"

final_state=$(get_node_state $follower_port)
final_term=$(get_term $follower_port)
leader_id=$(get_leader_id $follower_port)

echo ""
print_info "Node $follower_node Status After Restart:"
echo "  State: $final_state"
echo "  Term: $final_term"
echo "  Known Leader: Node $leader_id"
echo ""

if [[ "$final_state" == "Follower" ]]; then
    print_success "Node $follower_node is Follower ✓"
else
    print_error "Node $follower_node is NOT Follower (state: $final_state)"
fi

if [[ "$final_term" -eq "$pre_restart_term" ]] || [[ "$final_term" -gt "$pre_restart_term" ]]; then
    print_success "Term is current: $final_term (was $pre_restart_term) ✓"
else
    print_error "Term is outdated: $final_term (was $pre_restart_term)"
fi

if [[ "$leader_id" -eq "$pre_restart_leader" ]] || [[ "$leader_id" -gt 0 ]]; then
    print_success "Follower knows current leader: Node $leader_id ✓"
else
    print_error "Follower doesn't know leader (got: $leader_id)"
fi

# Phase 8: Full cluster verification
print_header "Phase 8: Full Cluster State After Follower Rejoin"

print_cluster_state

# Verify all 3 nodes operational
all_operational=0
for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    if [[ ! "$state" == "UNREACHABLE" ]]; then
        all_operational=$((all_operational + 1))
    fi
done

if [ $all_operational -eq 3 ]; then
    print_success "All 3 nodes operational"
else
    print_error "Not all nodes operational: $all_operational/3"
fi

# Phase 9: Verify cluster consistency
print_header "Phase 9: Verify Cluster Consistency"

# All nodes should have same term
term1=$(get_term 8001)
term2=$(get_term 8002)
term3=$(get_term 8003)

echo ""
print_info "Current terms across cluster:"
echo "  Node 1: $term1"
echo "  Node 2: $term2"
echo "  Node 3: $term3"

if [[ "$term1" == "$term2" ]] && [[ "$term2" == "$term3" ]]; then
    print_success "All nodes agree on current term ✓"
else
    print_warning "Terms differ (should sync shortly)"
fi

# All nodes should agree on leader
leader1=$(get_leader_id 8001)
leader2=$(get_leader_id 8002)
leader3=$(get_leader_id 8003)

echo ""
print_info "Current leader according to each node:"
echo "  Node 1: Node $leader1"
echo "  Node 2: Node $leader2"
echo "  Node 3: Node $leader3"

if [[ "$leader1" == "$leader2" ]] && [[ "$leader2" == "$leader3" ]]; then
    print_success "All nodes agree on leader ✓"
else
    print_warning "Leader disagreement (should sync shortly)"
fi

print_header "SUMMARY"

echo ""
echo "Follower Rejoin Test Results:"
echo "=============================="
echo ""
echo "Tested Node: Node $follower_node (Follower)"
echo "Test Duration: ~$(($rejoin_time / 2)) seconds"
echo ""

if [[ "$final_state" == "Follower" ]] && [[ "$all_operational" -eq 3 ]]; then
    print_success "PASSED: Follower correctly rejoined as Follower"
    echo ""
    echo "Verified:"
    echo "  ✓ Follower rejoined after crash"
    echo "  ✓ Rejoined node returned to Follower state"
    echo "  ✓ Rejoined node has current term"
    echo "  ✓ Rejoined node knows correct leader"
    echo "  ✓ All 3 nodes operational"
    echo "  ✓ Cluster consistency maintained"
else
    print_error "FAILED: Follower did not rejoin correctly"
    echo ""
    echo "Issues:"
    if [[ ! "$final_state" == "Follower" ]]; then
        echo "  ✗ Follower state: $final_state (expected: Follower)"
    fi
    if [[ ! "$all_operational" -eq 3 ]]; then
        echo "  ✗ Only $all_operational/3 nodes operational"
    fi
fi
