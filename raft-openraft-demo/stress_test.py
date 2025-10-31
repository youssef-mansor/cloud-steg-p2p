#!/usr/bin/env python3
"""
Stress Test for Clustered Load Balancer System
- Send thousands of requests
- Track metrics: latency, throughput, errors, failover
- Test with server failures
"""

import requests
import hashlib
import time
import random
import threading
import json
from collections import defaultdict
from datetime import datetime

class StressTestMetrics:
    """Track all metrics during stress test"""
    
    def __init__(self):
        self.start_time = None
        self.end_time = None
        
        # Request tracking
        self.total_requests = 0
        self.successful_requests = 0
        self.failed_requests = 0
        self.timeout_requests = 0
        
        # Latency tracking (in seconds)
        self.latencies = []
        self.min_latency = float('inf')
        self.max_latency = 0
        self.total_latency = 0
        
        # Error tracking
        self.errors = defaultdict(int)
        
        # LB distribution
        self.lb_requests = defaultdict(int)
        self.lb_latencies = defaultdict(list)
        
        # Server distribution (extracted from response headers)
        self.server_requests = defaultdict(int)
        self.server_latencies = defaultdict(list)
        self.server_errors = defaultdict(int)
        
        # Latency buckets for distribution
        self.latency_buckets = defaultdict(int)
        
        # Lock for thread safety
        self.lock = threading.Lock()
    
    def record_request(self, lb_id, latency, success, error=None, server_id=None):
        """Record a single request"""
        with self.lock:
            self.total_requests += 1
            self.lb_requests[lb_id] += 1
            self.lb_latencies[lb_id].append(latency)
            
            # Categorize latency
            if latency < 0.01:
                self.latency_buckets['<10ms'] += 1
            elif latency < 0.05:
                self.latency_buckets['10-50ms'] += 1
            elif latency < 0.1:
                self.latency_buckets['50-100ms'] += 1
            else:
                self.latency_buckets['>100ms'] += 1
            
            if server_id:
                self.server_requests[server_id] += 1
                self.server_latencies[server_id].append(latency)
            
            if success:
                self.successful_requests += 1
                self.latencies.append(latency)
                self.total_latency += latency
                self.min_latency = min(self.min_latency, latency)
                self.max_latency = max(self.max_latency, latency)
            else:
                self.failed_requests += 1
                if error:
                    self.errors[error] += 1
                if server_id:
                    self.server_errors[server_id] += 1
    
    def get_summary(self):
        """Generate summary report"""
        duration = self.end_time - self.start_time
        
        success_rate = (self.successful_requests / self.total_requests * 100) if self.total_requests > 0 else 0
        throughput = self.total_requests / duration if duration > 0 else 0
        
        avg_latency = self.total_latency / len(self.latencies) if self.latencies else 0
        
        # Calculate percentiles
        sorted_latencies = sorted(self.latencies)
        p50 = sorted_latencies[int(len(sorted_latencies) * 0.50)] if len(sorted_latencies) > 0 else 0
        p95 = sorted_latencies[int(len(sorted_latencies) * 0.95)] if len(sorted_latencies) > 0 else 0
        p99 = sorted_latencies[int(len(sorted_latencies) * 0.99)] if len(sorted_latencies) > 0 else 0
        
        # Per-LB metrics
        lb_metrics = {}
        for lb_id in sorted(self.lb_requests.keys()):
            latencies = self.lb_latencies[lb_id]
            if latencies:
                lb_metrics[f"LB{lb_id}"] = {
                    "requests": self.lb_requests[lb_id],
                    "avg_latency_ms": (sum(latencies) / len(latencies)) * 1000,
                    "min_latency_ms": min(latencies) * 1000,
                    "max_latency_ms": max(latencies) * 1000,
                }
        
        # Per-server metrics
        server_metrics = {}
        for server_id in sorted(self.server_requests.keys()):
            latencies = self.server_latencies[server_id]
            if latencies:
                server_metrics[f"Server{server_id}"] = {
                    "requests": self.server_requests[server_id],
                    "errors": self.server_errors[server_id],
                    "success_rate": ((self.server_requests[server_id] - self.server_errors[server_id]) / self.server_requests[server_id] * 100),
                    "avg_latency_ms": (sum(latencies) / len(latencies)) * 1000,
                    "min_latency_ms": min(latencies) * 1000,
                    "max_latency_ms": max(latencies) * 1000,
                }
        
        return {
            "duration_seconds": duration,
            "total_requests": self.total_requests,
            "successful_requests": self.successful_requests,
            "failed_requests": self.failed_requests,
            "success_rate_percent": success_rate,
            "throughput_rps": throughput,
            "latency": {
                "min_ms": self.min_latency * 1000,
                "max_ms": self.max_latency * 1000,
                "avg_ms": avg_latency * 1000,
                "p50_ms": p50 * 1000,
                "p95_ms": p95 * 1000,
                "p99_ms": p99 * 1000,
            },
            "lb_distribution": dict(self.lb_requests),
            "lb_metrics": lb_metrics,
            "server_metrics": server_metrics,
            "latency_distribution": dict(self.latency_buckets),
            "error_breakdown": dict(self.errors),
        }


class LoadBalancerStressTest:
    """Stress test runner"""
    
    LBS = [
        "http://127.0.0.1:9001",
        "http://127.0.0.1:9002",
        "http://127.0.0.1:9003",
    ]
    
    def __init__(self, num_requests=5000, num_threads=10, image_size_kb=100):
        self.num_requests = num_requests
        self.num_threads = num_threads
        self.image_size_kb = image_size_kb
        self.metrics = StressTestMetrics()
        
        # Create test image
        self.test_data = b"Test image content " * (image_size_kb * 50)
        self.test_hash = hashlib.md5(self.test_data).hexdigest()
        
        print(f"🧪 Stress Test Configuration")
        print(f"   Total requests: {num_requests:,}")
        print(f"   Concurrent threads: {num_threads}")
        print(f"   Image size: {image_size_kb}KB")
        print(f"   Test data hash: {self.test_hash}")
        print(f"   LBs: {self.LBS}")
        print()
    
    def worker(self, thread_id, requests_per_thread):
        """Worker thread that sends requests with failover"""
        for i in range(requests_per_thread):
            # Try all LBs until one works
            lbs_to_try = self.LBS.copy()
            random.shuffle(lbs_to_try)
            
            for attempt, lb in enumerate(lbs_to_try):
                lb_id = self.LBS.index(lb) + 1
                detected_server_id = None
                
                try:
                    start = time.time()
                    response = requests.post(
                        f"{lb}/image/echo",
                        data=self.test_data,
                        headers={'Content-Type': 'application/octet-stream'},
                        timeout=5
                    )
                    latency = time.time() - start
                    
                    if response.status_code == 200:
                        # Extract which server handled it
                        detected_server_id = response.headers.get('X-Backend-Server')
                        if detected_server_id:
                            detected_server_id = int(detected_server_id)
                        
                        returned_hash = hashlib.md5(response.content).hexdigest()
                        success = (returned_hash == self.test_hash)
                        self.metrics.record_request(lb_id, latency, success, server_id=detected_server_id)
                        break
                    else:
                        self.metrics.record_request(lb_id, latency, False, f"HTTP_{response.status_code}")
                
                except requests.exceptions.Timeout:
                    if attempt < len(lbs_to_try) - 1:
                        continue
                    self.metrics.record_request(lb_id, 5.0, False, "Timeout")
                
                except requests.exceptions.ConnectionError:
                    if attempt < len(lbs_to_try) - 1:
                        continue
                    self.metrics.record_request(lb_id, 0, False, "AllLBsFailed")
                
                except Exception as e:
                    if attempt < len(lbs_to_try) - 1:
                        continue
                    self.metrics.record_request(lb_id, 0, False, type(e).__name__)
    
    def run(self):
        """Run the stress test with real-time monitoring"""
        print(f"🚀 Starting stress test at {datetime.now().strftime('%H:%M:%S')}\n")
        
        self.metrics.start_time = time.time()
        
        # Calculate requests per thread
        requests_per_thread = self.num_requests // self.num_threads
        
        # Start threads
        threads = []
        for i in range(self.num_threads):
            t = threading.Thread(target=self.worker, args=(i, requests_per_thread))
            t.daemon = False
            threads.append(t)
            t.start()
        
        print(f"  Threads running: {len(threads)}")
        print(f"  Monitoring progress...\n")
        
        # Monitor progress
        last_completed = 0
        while any(t.is_alive() for t in threads):
            time.sleep(1)
            
            completed = self.metrics.total_requests
            success = self.metrics.successful_requests
            failed = self.metrics.failed_requests
            elapsed = time.time() - self.metrics.start_time
            throughput = completed / elapsed if elapsed > 0 else 0
            
            # Estimate time remaining
            if throughput > 0:
                remaining = (self.num_requests - completed) / throughput
                eta = f"ETA: {int(remaining)}s"
            else:
                eta = "ETA: --"
            
            # Progress bar
            percent = (completed / self.num_requests * 100) if self.num_requests > 0 else 0
            bar_length = 30
            filled = int(bar_length * percent / 100)
            bar = "█" * filled + "░" * (bar_length - filled)
            
            print(f"  [{bar}] {percent:6.1f}% | {completed:,}/{self.num_requests:,} | {throughput:.0f} req/sec | {success:,} ok, {failed:,} failed | {eta}")
            
            last_completed = completed
        
        # Wait for all threads
        for t in threads:
            t.join()
        
        self.metrics.end_time = time.time()
        
        print()
        self.print_results()
    
    def print_results(self):
        """Print comprehensive results"""
        summary = self.metrics.get_summary()
        
        print("=" * 80)
        print("📊 STRESS TEST RESULTS")
        print("=" * 80)
        print()
        
        print(f"⏱️  Test Duration: {summary['duration_seconds']:.2f} seconds")
        print()
        
        print("📈 Overall Throughput & Success:")
        print(f"   Total Requests: {summary['total_requests']:,}")
        print(f"   Successful: {summary['successful_requests']:,}")
        print(f"   Failed: {summary['failed_requests']:,}")
        print(f"   Success Rate: {summary['success_rate_percent']:.2f}%")
        print(f"   Throughput: {summary['throughput_rps']:.2f} req/sec")
        print()
        
        print("⏳ Overall Latency (milliseconds):")
        print(f"   Min: {summary['latency']['min_ms']:.2f}ms")
        print(f"   Avg: {summary['latency']['avg_ms']:.2f}ms")
        print(f"   P50: {summary['latency']['p50_ms']:.2f}ms")
        print(f"   P95: {summary['latency']['p95_ms']:.2f}ms")
        print(f"   P99: {summary['latency']['p99_ms']:.2f}ms")
        print(f"   Max: {summary['latency']['max_ms']:.2f}ms")
        print()
        
        print("⚖️  Load Balancer Distribution:")
        for lb_id, count in sorted(summary['lb_distribution'].items()):
            percent = (count / summary['total_requests'] * 100)
            print(f"   LB{lb_id}: {count:,} ({percent:.1f}%)")
        print()
        
        if summary['lb_metrics']:
            print("📊 Per-LB Metrics:")
            for lb_name, metrics in summary['lb_metrics'].items():
                print(f"   {lb_name}:")
                print(f"      Requests: {metrics['requests']:,}")
                print(f"      Avg Latency: {metrics['avg_latency_ms']:.2f}ms")
                print(f"      Min/Max: {metrics['min_latency_ms']:.2f}ms / {metrics['max_latency_ms']:.2f}ms")
            print()
        
        if summary['server_metrics']:
            print("🖥️  Per-Server Metrics:")
            for server_name, metrics in summary['server_metrics'].items():
                print(f"   {server_name}:")
                print(f"      Requests: {metrics['requests']:,}")
                print(f"      Success Rate: {metrics['success_rate']:.2f}%")
                print(f"      Avg Latency: {metrics['avg_latency_ms']:.2f}ms")
                print(f"      Min/Max: {metrics['min_latency_ms']:.2f}ms / {metrics['max_latency_ms']:.2f}ms")
            print()
        
        # ← NEW: Enhanced statistics
        print("📊 Additional Statistics:")
        
        # Load balance deviation
        if summary['lb_distribution']:
            lb_counts = list(summary['lb_distribution'].values())
            avg_per_lb = sum(lb_counts) / len(lb_counts)
            if avg_per_lb > 0:
                max_deviation = max(abs(x - avg_per_lb) for x in lb_counts) / avg_per_lb * 100
                print(f"   LB Load Balance Deviation: {max_deviation:.1f}% (ideal: 0%)")
        
        if summary['server_metrics']:
            server_counts = [m['requests'] for m in summary['server_metrics'].values()]
            if server_counts:
                avg_per_server = sum(server_counts) / len(server_counts)
                if avg_per_server > 0:
                    max_deviation = max(abs(x - avg_per_server) for x in server_counts) / avg_per_server * 100
                    print(f"   Server Load Balance Deviation: {max_deviation:.1f}% (ideal: 0%)")
        
        # Latency distribution
        if summary['latency_distribution']:
            print(f"\n   Latency Distribution:")
            for bucket in ['<10ms', '10-50ms', '50-100ms', '>100ms']:
                count = summary['latency_distribution'].get(bucket, 0)
                percent = (count / summary['total_requests'] * 100) if summary['total_requests'] > 0 else 0
                print(f"     {bucket:>10}: {count:>6} ({percent:>5.1f}%)")
        
        # Data transferred
        total_data_mb = (summary['successful_requests'] * self.image_size_kb) / 1024
        print(f"\n   Total Data Transferred: {total_data_mb:.1f} MB")
        
        if summary['error_breakdown']:
            print(f"\n❌ Error Breakdown:")
            for error, count in sorted(summary['error_breakdown'].items(), key=lambda x: x[1], reverse=True):
                print(f"   {error}: {count}")
        
        print()
        print("=" * 80)
        
        # Save results to file
        results_file = f"stress_test_results_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
        with open(results_file, 'w') as f:
            json.dump(summary, f, indent=2)
        print(f"✅ Results saved to {results_file}")


def run_with_server_failure(num_requests=20000):
    """Run stress test with server failure mid-test"""
    
    print("🔥 STRESS TEST WITH SERVER FAILURE INJECTION")
    print()
    print("   Configuration:")
    print("   • Total requests: 20,000")
    print("   • Estimated duration: ~30 seconds")
    print("   • Server 1 will be killed ~10 seconds in")
    print()
    print("   Setup: Have all 3 servers + 3 LBs ready")
    print()
    print("   Press Enter to start...")
    input()
    
    print()
    print("🚀 Test starting in 3 seconds...")
    time.sleep(3)
    
    test = LoadBalancerStressTest(
        num_requests=num_requests,
        num_threads=20,
        image_size_kb=100
    )
    
    # Start test
    test_thread = threading.Thread(target=test.run)
    test_thread.start()
    
    # Wait 10 seconds
    print()
    print("   [Waiting for test to stabilize...]")
    time.sleep(10)
    
    print()
    print("   ⚠️  KILL SERVER 1 NOW! (Ctrl+C in Terminal 1)")
    print()
    print("   Then press Enter to continue...")
    input()
    
    print()
    print("   ✅ Continuing with Server 1 down...")
    print()
    
    # Wait for completion
    test_thread.join()


if __name__ == "__main__":
    import sys
    
    print("Choose test mode:")
    print("  1) Normal stress test (5000 requests)")
    print("  2) Stress test with server failure")
    print("  3) High throughput (10000 requests, 20 threads)")
    print()
    
    choice = input("Enter choice (1-3): ").strip()
    
    if choice == "1":
        test = LoadBalancerStressTest(num_requests=5000, num_threads=10, image_size_kb=100)
        test.run()
    
    elif choice == "2":
        run_with_server_failure(num_requests=20000)
    
    elif choice == "3":
        test = LoadBalancerStressTest(num_requests=10000, num_threads=20, image_size_kb=100)
        test.run()
    
    else:
        print("Invalid choice")
