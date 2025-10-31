#!/bin/bash
set -e

TOTAL_REQUESTS=2000 
BATCH_SIZE=100
OUTPUT_DIR="/tmp/load_balance_test_$$"
RESULTS_FILE="$OUTPUT_DIR/results.txt"
SUMMARY_FILE="$OUTPUT_DIR/summary.txt"

echo "🚀 Load Balancing Test - $TOTAL_REQUESTS Requests"
echo "=================================================="
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if input image exists
if [ ! -f "input-image.png" ]; then
    echo "❌ Error: input-image.png not found"
    echo "   Create a test image or use an existing one"
    exit 1
fi

# Find which node is the leader
echo "🔍 Checking which node is the leader..."
LEADER_NODE=$(curl -s http://127.0.0.1:8001/metrics | jq -r '.data.current_leader // empty')
if [ -z "$LEADER_NODE" ]; then
    echo "❌ Error: Could not find leader. Make sure all 3 nodes are running!"
    exit 1
fi

echo "✅ Leader is Node $LEADER_NODE"
echo "📡 Will multicast requests to all 3 nodes (port 8001, 8002, 8003)"
echo "   - Followers will reject (503)"
echo "   - Leader will forward to healthy nodes"
echo ""

# Initialize counters (using regular variables for compatibility)
node_count_1=0
node_count_2=0
node_count_3=0
success_count=0
failure_count=0

echo "📤 Sending $TOTAL_REQUESTS requests..."
echo "   This may take a few minutes..."
echo ""

# Multicast request to all 3 nodes - followers drop, leader processes
multicast_request() {
    local request_num=$1
    local max_retries=2
    local retry_count=0
    local HTTP_CODE="000"
    local PROCESSED_BY=""
    
    # Retry loop - multicast again on failure
    while [ $retry_count -le $max_retries ]; do
        # Multicast to all 3 nodes simultaneously (background)
        local headers_file_1="$OUTPUT_DIR/h_${request_num}_1"
        local headers_file_2="$OUTPUT_DIR/h_${request_num}_2"
        local headers_file_3="$OUTPUT_DIR/h_${request_num}_3"
        local body_file_1="$OUTPUT_DIR/b_${request_num}_1"
        local body_file_2="$OUTPUT_DIR/b_${request_num}_2"
        local body_file_3="$OUTPUT_DIR/b_${request_num}_3"
        
        # Send to all 3 nodes in parallel
        curl -s --max-time 10 -o "$body_file_1" -w "%{http_code}" \
            -X POST "http://127.0.0.1:8001/image/echo" \
            --data-binary @input-image.png \
            -D "$headers_file_1" 2>/dev/null > "$OUTPUT_DIR/code_${request_num}_1" &
        PID1=$!
        
        curl -s --max-time 10 -o "$body_file_2" -w "%{http_code}" \
            -X POST "http://127.0.0.1:8002/image/echo" \
            --data-binary @input-image.png \
            -D "$headers_file_2" 2>/dev/null > "$OUTPUT_DIR/code_${request_num}_2" &
        PID2=$!
        
        curl -s --max-time 10 -o "$body_file_3" -w "%{http_code}" \
            -X POST "http://127.0.0.1:8003/image/echo" \
            --data-binary @input-image.png \
            -D "$headers_file_3" 2>/dev/null > "$OUTPUT_DIR/code_${request_num}_3" &
        PID3=$!
        
        # Wait for all to complete
        wait $PID1 $PID2 $PID3
        
        # Check which node responded successfully (leader or forwarded follower)
        local code1=$(cat "$OUTPUT_DIR/code_${request_num}_1" 2>/dev/null || echo "000")
        local code2=$(cat "$OUTPUT_DIR/code_${request_num}_2" 2>/dev/null || echo "000")
        local code3=$(cat "$OUTPUT_DIR/code_${request_num}_3" 2>/dev/null || echo "000")
        
        # Find the first successful response (200)
        if [ "$code1" = "200" ]; then
            HTTP_CODE="$code1"
            PROCESSED_BY=$(grep -i "^x-processed-by-node:" "$headers_file_1" 2>/dev/null | \
                sed 's/^[^:]*:[[:space:]]*//' | tr -d '\r\n' || echo "")
            break
        elif [ "$code2" = "200" ]; then
            HTTP_CODE="$code2"
            PROCESSED_BY=$(grep -i "^x-processed-by-node:" "$headers_file_2" 2>/dev/null | \
                sed 's/^[^:]*:[[:space:]]*//' | tr -d '\r\n' || echo "")
            break
        elif [ "$code3" = "200" ]; then
            HTTP_CODE="$code3"
            PROCESSED_BY=$(grep -i "^x-processed-by-node:" "$headers_file_3" 2>/dev/null | \
                sed 's/^[^:]*:[[:space:]]*//' | tr -d '\r\n' || echo "")
            break
        fi
        
        # If all failed and we have retries left, wait and multicast again
        if [ $retry_count -lt $max_retries ]; then
            retry_count=$((retry_count + 1))
            sleep 0.2  # Small delay before retry
        else
            break
        fi
        
        # Cleanup for retry
        rm -f "$headers_file_1" "$headers_file_2" "$headers_file_3" \
              "$body_file_1" "$body_file_2" "$body_file_3" \
              "$OUTPUT_DIR/code_${request_num}_1" "$OUTPUT_DIR/code_${request_num}_2" "$OUTPUT_DIR/code_${request_num}_3"
    done
    
    # Cleanup
    rm -f "$headers_file_1" "$headers_file_2" "$headers_file_3" \
          "$body_file_1" "$body_file_2" "$body_file_3" \
          "$OUTPUT_DIR/code_${request_num}_1" "$OUTPUT_DIR/code_${request_num}_2" "$OUTPUT_DIR/code_${request_num}_3"
    
    if [ "$HTTP_CODE" = "200" ]; then
        success_count=$((success_count + 1))
        if [ -n "$PROCESSED_BY" ]; then
            case "$PROCESSED_BY" in
                1) node_count_1=$((node_count_1 + 1)) ;;
                2) node_count_2=$((node_count_2 + 1)) ;;
                3) node_count_3=$((node_count_3 + 1)) ;;
            esac
            if [ $retry_count -gt 0 ]; then
                echo "$request_num\t$HTTP_CODE\t$PROCESSED_BY\tretries:$retry_count" >> "$RESULTS_FILE"
            else
                echo "$request_num\t$HTTP_CODE\t$PROCESSED_BY" >> "$RESULTS_FILE"
            fi
        else
            echo "$request_num\t$HTTP_CODE\tunknown" >> "$RESULTS_FILE"
        fi
    else
        failure_count=$((failure_count + 1))
        echo "$request_num\t$HTTP_CODE\tfailed\tretries:$retry_count" >> "$RESULTS_FILE"
    fi
    
    # Progress indicator
    if [ $((request_num % 100)) -eq 0 ]; then
        echo -n "."
    fi
    if [ $((request_num % 1000)) -eq 0 ]; then
        if [ -n "$start_time" ]; then
            local elapsed=$(($(date +%s) - start_time))
            echo " $request_num/$TOTAL_REQUESTS (${elapsed}s elapsed)"
        else
            echo " $request_num/$TOTAL_REQUESTS"
        fi
    fi
}

# Start time
start_time=$(date +%s)

# Send all requests (multicasting to all 3 nodes)
echo "Sending requests (multicasting to all 3 nodes)..."
for i in $(seq 1 $TOTAL_REQUESTS); do
    multicast_request $i
    
    # Small delay to avoid overwhelming the server
    if [ $((i % 50)) -eq 0 ]; then
        sleep 0.01
    fi
done

echo ""
echo ""

# Calculate statistics
total_processed=$((node_count_1 + node_count_2 + node_count_3))
if [ $total_processed -eq 0 ]; then
    total_processed=1  # Avoid division by zero
fi

# Generate summary
{
    echo "📊 Load Balancing Test Summary"
    echo "=============================="
    echo ""
    echo "Total Requests Sent: $TOTAL_REQUESTS"
    echo "Successful Requests: $success_count"
    echo "Failed Requests: $failure_count"
    echo ""
    echo "Architecture:"
    echo "  • Requests multicast to all 3 nodes (port 8001, 8002, 8003)"
    echo "  • Followers reject direct requests (503)"
    echo "  • Leader forwards to healthy nodes only"
    echo "  • Client-side retry: multicast again on failure (up to 2 retries)"
    echo "  • 5-second timeout on node-to-node forwarding"
    echo "  • 10-second timeout on client requests"
    echo ""
    echo "Requests Processed by Each Node:"
    echo "---------------------------------"
    for node_id in 1 2 3; do
        case "$node_id" in
            1) count=$node_count_1 ;;
            2) count=$node_count_2 ;;
            3) count=$node_count_3 ;;
        esac
        if command -v bc &> /dev/null; then
            percentage=$(echo "scale=2; $count * 100 / $total_processed" | bc)
        else
            percentage=$(awk "BEGIN {printf \"%.2f\", $count * 100 / $total_processed}")
        fi
        echo "  Node $node_id: $count requests ($percentage%)"
    done
    echo ""
    echo "Distribution Analysis:"
    echo "----------------------"
    
    # Calculate expected distribution (should be ~33.33% each)
    # Check if bc is available, otherwise use awk
    if command -v bc &> /dev/null; then
        expected_percent=33.33
        for node_id in 1 2 3; do
            case "$node_id" in
                1) count=$node_count_1 ;;
                2) count=$node_count_2 ;;
                3) count=$node_count_3 ;;
            esac
            percentage=$(echo "scale=2; $count * 100 / $total_processed" | bc)
            diff=$(echo "scale=2; $percentage - $expected_percent" | bc)
            if [ $(echo "$diff > 0" | bc) -eq 1 ]; then
                echo "  Node $node_id: +$diff% above expected"
            else
                diff_abs=$(echo "scale=2; $diff * -1" | bc)
                echo "  Node $node_id: -$diff_abs% below expected"
            fi
        done
    else
        echo "  (Install 'bc' for detailed distribution analysis)"
    fi
    
    echo ""
    if [ -n "$start_time" ]; then
        elapsed=$(($(date +%s) - start_time))
        echo "Total Time: ${elapsed} seconds"
        if command -v bc &> /dev/null; then
            echo "Average Throughput: $(echo "scale=2; $TOTAL_REQUESTS / $elapsed" | bc) requests/second"
        else
            throughput=$(awk "BEGIN {printf \"%.2f\", $TOTAL_REQUESTS / $elapsed}")
            echo "Average Throughput: $throughput requests/second"
        fi
    fi
    
    echo ""
    echo "Detailed results saved to: $RESULTS_FILE"
} | tee "$SUMMARY_FILE"

echo ""
echo "✅ Test Complete!"
echo ""
echo "Summary saved to: $SUMMARY_FILE"
echo "Detailed results saved to: $RESULTS_FILE"
echo ""

