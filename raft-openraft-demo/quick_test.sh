#!/bin/bash

# FIXED Quick Reference: Manual Election and Fault Tolerance Tests
# Corrected JSON parsing to match actual /metrics response

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_help() {
    cat << 'EOF'
Raft Cluster Manual Testing Quick Reference (FIXED)

Usage: ./quick_test_FIXED.sh [command]

Commands:
  status              - Show current state of all 3 nodes
  leader              - Show who is the current leader
  terms               - Show current term for each node
  metrics NODE        - Show detailed metrics for node (1, 2, or 3)

  kill-leader         - Stop the current leader node
  kill-node NODE      - Stop node 1, 2, or 3

  wait-election       - Wait and show new leader election
  wait-rejoin DELAY   - Wait for node to rejoin after DELAY seconds

  echo-test           - Test the /image/echo endpoint on leader

  Examples:
    ./quick_test_FIXED.sh status           # Check all nodes
    ./quick_test_FIXED.sh leader           # Who's the leader?
    ./quick_test_FIXED.sh metrics 1        # Details of Node 1
    ./quick_test_FIXED.sh kill-leader      # Kill current leader
    ./quick_test_FIXED.sh wait-election    # Monitor election

EOF
    exit 0
}

# Color wrappers
success() { echo -e "${GREEN}✅ $1${NC}"; }
error() { echo -e "${RED}❌ $1${NC}"; }
info() { echo -e "${BLUE}ℹ️  $1${NC}"; }
warn() { echo -e "${YELLOW}⚠️  $1${NC}"; }

# FIXED: Helper functions with correct JSON parsing
get_node_metrics() {
    local port=$1
    curl -s http://127.0.0.1:$port/metrics 2>/dev/null || echo "{}"
}

get_node_state() {
    local port=$1
    local metrics=$(get_node_metrics $port)
    echo "$metrics" | jq -r '.data.state // "UNREACHABLE"' 2>/dev/null
}

# FIXED: Use current_leader, not leader_id
get_leader_id() {
    local port=$1
    local metrics=$(get_node_metrics $port)
    echo "$metrics" | jq -r '.data.current_leader // "none"' 2>/dev/null
}

get_term() {
    local port=$1
    local metrics=$(get_node_metrics $port)
    echo "$metrics" | jq -r '.data.current_term // "?"' 2>/dev/null
}

get_node_id() {
    local port=$1
    local metrics=$(get_node_metrics $port)
    echo "$metrics" | jq -r '.data.node_id // "?"' 2>/dev/null
}

# Commands

cmd_status() {
    echo ""
    info "Cluster Status (Current Time: $(date '+%H:%M:%S'))"
    echo ""

    for port in 8001 8002 8003; do
        node_id=$((port - 8000))
        state=$(get_node_state $port)
        leader=$(get_leader_id $port)
        term=$(get_term $port)

        printf "  Node %d: " $node_id

        if [[ "$state" == "UNREACHABLE" ]]; then
            echo "${RED}[UNREACHABLE]${NC}"
        else
            printf "${GREEN}%-10s${NC} term=%-2s leader=%-2s\n" "$state" "$term" "$leader"
        fi
    done
    echo ""
}

cmd_leader() {
    echo ""
    for port in 8001 8002 8003; do
        node_id=$((port - 8000))
        state=$(get_node_state $port)

        if [[ "$state" == "Leader" ]]; then
            term=$(get_term $port)
            success "Current leader: Node $node_id (term: $term)"
            echo ""
            return 0
        fi
    done

    error "No leader found!"
    echo ""
}

cmd_terms() {
    echo ""
    info "Current Terms"
    for port in 8001 8002 8003; do
        node_id=$((port - 8000))
        state=$(get_node_state $port)
        term=$(get_term $port)

        if [[ "$state" == "UNREACHABLE" ]]; then
            printf "  Node %d: UNREACHABLE\n" $node_id
        else
            printf "  Node %d: term=%s (%s)\n" $node_id $term "$state"
        fi
    done
    echo ""
}

cmd_metrics() {
    local node=$1
    if [[ -z "$node" ]]; then
        error "Please specify node (1, 2, or 3)"
        exit 1
    fi

    port=$((8000 + node))
    echo ""
    info "Detailed Metrics for Node $node"

    metrics=$(get_node_metrics $port)

    if [[ "$metrics" == "{}" ]]; then
        error "Node $node unreachable on port $port"
    else
        echo "$metrics" | jq '.data' 2>/dev/null || echo "$metrics"
    fi
    echo ""
}

cmd_kill_leader() {
    # Find the leader
    for port in 8001 8002 8003; do
        state=$(get_node_state $port)
        if [[ "$state" == "Leader" ]]; then
            node_id=$((port - 8000))
            cmd_kill_node $node_id
            return
        fi
    done

    error "No leader found to kill"
}

cmd_kill_node() {
    local node=$1
    if [[ -z "$node" ]]; then
        error "Please specify node (1, 2, or 3)"
        exit 1
    fi

    warn "To kill Node $node, run in its terminal:"
    echo "    Ctrl+C"
    echo ""
    info "Or in another terminal:"
    echo "    pkill -f 'cargo run.*--id $node'"
    echo ""
}

cmd_wait_election() {
    echo ""
    info "Monitoring for leader election (Ctrl+C to stop)"

    previous_leader=""
    counter=0

    while true; do
        current_leader=""
        current_term=""

        # Check all nodes for a leader
        for port in 8001 8002 8003; do
            state=$(get_node_state $port)
            if [[ "$state" == "Leader" ]]; then
                current_leader=$((port - 8000))
                current_term=$(get_term $port)
                break
            fi
        done

        timestamp=$(date '+%H:%M:%S')

        if [[ -z "$current_leader" ]]; then
            printf "\r[$timestamp] No leader elected yet (waiting...)       "
        else
            if [[ "$current_leader" != "$previous_leader" ]]; then
                echo ""
                success "New leader elected: Node $current_leader (term=$current_term)"
                previous_leader=$current_leader
            fi
            printf "\r[$timestamp] Leader: Node $current_leader (term=$current_term)        "
        fi

        counter=$((counter + 1))
        if [ $counter -gt 120 ]; then  # 2 minutes timeout
            echo ""
            info "Timeout after 120 seconds of monitoring"
            break
        fi

        sleep 0.5
    done
    echo ""
}

cmd_wait_rejoin() {
    local delay=${1:-3}
    echo ""
    info "Waiting $delay seconds, then monitoring for rejoin..."

    sleep $delay

    info "Monitoring rejoin (Ctrl+C to stop)"
    counter=0
    rejoin_detected=0

    while true; do
        healthy_nodes=0
        for port in 8001 8002 8003; do
            state=$(get_node_state $port)
            if [[ ! $state == *"UNREACHABLE"* ]]; then
                healthy_nodes=$((healthy_nodes + 1))
            fi
        done

        timestamp=$(date '+%H:%M:%S')

        if [ $healthy_nodes -eq 3 ]; then
            if [ $rejoin_detected -eq 0 ]; then
                echo ""
                success "All 3 nodes operational!"
                rejoin_detected=1
            fi
            printf "\r[$timestamp] Status: 3/3 nodes healthy   "
        else
            printf "\r[$timestamp] Status: $healthy_nodes/3 nodes healthy     "
        fi

        counter=$((counter + 1))
        if [ $counter -gt 120 ]; then  # 2 minutes timeout
            echo ""
            info "Timeout after 120 seconds of monitoring"
            break
        fi

        sleep 0.5
    done
    echo ""
}

cmd_echo_test() {
    echo ""
    # Find leader
    leader_port=""
    for port in 8001 8002 8003; do
        state=$(get_node_state $port)
        if [[ "$state" == "Leader" ]]; then
            leader_port=$port
            break
        fi
    done

    if [[ -z "$leader_port" ]]; then
        error "No leader found"
        exit 1
    fi

    leader_id=$((leader_port - 8000))
    info "Testing /image/echo on leader (Node $leader_id)"

    test_data="Hello from Raft test!"
    response=$(curl -s -X POST http://127.0.0.1:$leader_port/image/echo \
      -H 'Content-Type: application/octet-stream' \
      -d "$test_data")

    if [[ "$response" == "$test_data" ]]; then
        success "Echo test passed!"
        echo "  Request:  $test_data"
        echo "  Response: $response"
    else
        error "Echo test failed!"
        echo "  Request:  $test_data"
        echo "  Response: $response"
    fi
    echo ""
}

# Main
if [[ $# -eq 0 ]]; then
    print_help
fi

case "$1" in
    status)
        cmd_status
        ;;
    leader)
        cmd_leader
        ;;
    terms)
        cmd_terms
        ;;
    metrics)
        cmd_metrics "$2"
        ;;
    kill-leader)
        cmd_kill_leader
        ;;
    kill-node)
        cmd_kill_node "$2"
        ;;
    wait-election)
        cmd_wait_election
        ;;
    wait-rejoin)
        cmd_wait_rejoin "$2"
        ;;
    echo-test)
        cmd_echo_test
        ;;
    help|-h|--help)
        print_help
        ;;
    *)
        error "Unknown command: $1"
        echo ""
        print_help
        exit 1
        ;;
esac
