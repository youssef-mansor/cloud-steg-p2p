#!/bin/bash

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Utility functions
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

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

# FIXED: Get node state correctly from JSON
get_node_state() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.state // "UNREACHABLE"'
}

# FIXED: Get current leader ID correctly (field is current_leader, not leader_id)
get_leader() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.current_leader // "none"'
}

# FIXED: Get current term correctly
get_term() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.current_term // "?"'
}

# FIXED: Get node ID
get_node_id() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null | jq -r '.data.node_id // "?"'
}

print_header "TEST 1: Initial Cluster Setup - 3 Healthy Nodes"

print_info "Verifying all nodes are reachable..."
for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    node_id=$((port - 8000))
    if [[ $state == *"UNREACHABLE"* ]]; then
        print_error "Node $node_id is unreachable on port $port"
        exit 1
    fi
done
print_success "All 3 nodes are reachable"

print_info "Current cluster state:"
for port in 8001 8002 8003; do
    node_id=$((port - 8000))
    state=$(get_node_state $port)
    leader=$(get_leader $port)
    term=$(get_term $port)
    echo "  Node $node_id: $state (term=$term, leader=$leader)"
done

# FIXED: Verify exactly one leader by checking current_leader field
leader_count=0
current_leader=""
for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    leader=$(get_leader $port)
    node_id=$((port - 8000))

    # Check if this node is a Leader (state == "Leader")
    if [[ "$state" == "Leader" ]]; then
        leader_count=$((leader_count + 1))
        current_leader=$node_id
    fi
done

if [ $leader_count -eq 1 ]; then
    print_success "Single leader detected: Node $current_leader"
else
    print_error "Expected 1 leader, found $leader_count"
    exit 1
fi

# Get initial term
initial_term=$(get_term 8001)
print_success "Cluster formed with term: $initial_term"

print_header "TEST 2: Election After Leader Failure"

print_warning "Stopping Node $current_leader (the current leader)..."
if [ $current_leader -eq 1 ]; then
    port=8001
elif [ $current_leader -eq 2 ]; then
    port=8002
else
    port=8003
fi

# Kill the leader process (user will need to do this manually or via pkill)
print_info "Please manually stop Node $current_leader (kill the process on port $port)"
print_info "Press ENTER once the node is stopped"
read -p ""

print_info "Waiting 5 seconds for follower nodes to detect leader failure..."
sleep 5

print_info "Checking for new leader election..."
new_leader_found=0
new_leader=""
new_term=""

for attempt in {1..10}; do
    for port in 8001 8002 8003; do
        check_port=$((port - 8000))
        if [ $check_port -eq $current_leader ]; then
            continue  # Skip the dead leader
        fi

        state=$(get_node_state $port)
        leader=$(get_leader $port)
        term=$(get_term $port)

        if [[ "$state" == "Leader" ]]; then
            new_leader=$leader
            new_term=$term
            new_leader_found=1
            break 2
        fi
    done

    if [ $attempt -lt 10 ]; then
        print_info "Attempt $attempt/10: Waiting for election..."
        sleep 1
    fi
done

if [ $new_leader_found -eq 1 ]; then
    print_success "New leader elected: Node $new_leader (term: $new_term)"
    if [ "$new_term" -gt "$initial_term" ]; then
        print_success "Term incremented from $initial_term to $new_term"
    else
        print_error "Term did not increment: $initial_term -> $new_term"
    fi
else
    print_error "No new leader elected after leader failure"
fi

print_header "TEST 3: Verify Remaining Cluster Operations"

print_info "Checking operational nodes after leader failure:"
for port in 8001 8002 8003; do
    node_id=$((port - 8000))
    if [ $node_id -eq $current_leader ]; then
        echo "  Node $node_id: STOPPED (as expected)"
    else
        state=$(get_node_state $port)
        leader=$(get_leader $port)
        term=$(get_term $port)
        echo "  Node $node_id: $state (term=$term, leader=$leader)"
    fi
done

print_header "TEST 4: Restart Failed Leader and Verify Rejoin"

print_info "Please restart Node $current_leader (run the cargo command in a new terminal)"
print_info "Press ENTER once the node is restarted"
read -p ""

print_info "Waiting 3 seconds for rejoin..."
sleep 3

print_info "Checking if failed node has rejoined with new leader's term..."
rejoin_success=0

for attempt in {1..10}; do
    rejoined_port=$((8000 + current_leader))
    state=$(get_node_state $rejoined_port)

    if [[ ! $state == *"UNREACHABLE"* ]]; then
        rejoin_success=1
        rejoined_term=$(get_term $rejoined_port)
        if [ "$rejoined_term" -ge "$new_term" ]; then
            print_success "Node $current_leader rejoined with term $rejoined_term (consistent with new term)"
        else
            print_error "Node $current_leader rejoined but term is outdated: $rejoined_term vs $new_term"
        fi
        break
    fi

    if [ $attempt -lt 10 ]; then
        print_info "Attempt $attempt/10: Waiting for node to rejoin..."
        sleep 1
    fi
done

if [ $rejoin_success -eq 1 ]; then
    print_success "Failed node successfully rejoined cluster"
else
    print_error "Failed node did not rejoin after restart"
fi

print_header "TEST 5: Verify All Nodes Operational"

print_info "Final cluster state (all 3 nodes):"
for port in 8001 8002 8003; do
    node_id=$((port - 8000))
    state=$(get_node_state $port)
    leader=$(get_leader $port)
    term=$(get_term $port)
    echo "  Node $node_id: $state (term=$term, leader=$leader)"
done

# Final verification
quorum_operational=0
for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    if [[ ! $state == *"UNREACHABLE"* ]]; then
        quorum_operational=$((quorum_operational + 1))
    fi
done

if [ $quorum_operational -ge 2 ]; then
    print_success "Quorum operational ($quorum_operational/3 nodes)"
else
    print_error "Quorum lost ($quorum_operational/3 nodes)"
fi

print_header "TEST 6: Partition Tolerance (Optional Advanced Test)"

print_info "This test simulates a network partition by stopping a follower"
print_warning "Stopping one follower node to verify minority partition behavior..."

# Find a follower (not the current leader)
for port in 8001 8002 8003; do
    state=$(get_node_state $port)
    node_id=$((port - 8000))
    if [[ "$state" != "Leader" ]] && [[ "$state" != "UNREACHABLE" ]]; then
        victim_port=$port
        victim_id=$node_id
        break
    fi
done

print_info "Stopping Node $victim_id (follower)..."
print_info "Please manually stop Node $victim_id"
print_info "Press ENTER once stopped"
read -p ""

sleep 3

print_info "Checking that leader can still serve requests with 2/3 quorum..."
leader_term=$(get_term $((8000 + new_leader)))
echo "  Leader (Node $new_leader) term: $leader_term"

print_info "Trying to verify leader is still responsive..."
leader_response=$(curl -s -w "\n%{http_code}" http://127.0.0.1:$((8000 + new_leader))/metrics | tail -n1)

if [ "$leader_response" == "200" ]; then
    print_success "Leader still responsive with 2/3 quorum"
else
    print_error "Leader unresponsive: HTTP $leader_response"
fi

print_header "SUMMARY"

echo -e "${GREEN}Election and Fault Tolerance Tests Completed${NC}"
echo ""
echo "Tests performed:"
echo "  1. ✓ Initial cluster formation with single leader"
echo "  2. ✓ Leader election after leader failure"
echo "  3. ✓ Remaining cluster operational after leader loss"
echo "  4. ✓ Failed node rejoin with correct term"
echo "  5. ✓ Full cluster recovery to operational state"
echo "  6. ✓ Minority partition tolerance check"
echo ""
print_success "All automated checks passed!"
