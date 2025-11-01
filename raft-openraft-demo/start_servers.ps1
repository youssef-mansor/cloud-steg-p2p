# Start three Raft nodes in background with output to console
$ErrorActionPreference = "Continue"

# Define node configurations
$nodes = @(
    @{id=1; http_port=8001; rpc_port=7001; peers="2=127.0.0.1:7002,3=127.0.0.1:7003"},
    @{id=2; http_port=8002; rpc_port=7002; peers="1=127.0.0.1:7001,3=127.0.0.1:7003"},
    @{id=3; http_port=8003; rpc_port=7003; peers="1=127.0.0.1:7001,2=127.0.0.1:7002"}
)

$binary = ".\target\release\raft-openraft-demo.exe"

Write-Host "Starting 3 Raft nodes..." -ForegroundColor Cyan

foreach ($node in $nodes) {
    $id = $node.id
    $http = $node.http_port
    $rpc = $node.rpc_port
    $peers = $node.peers
    
    Write-Host "Starting Node $id (HTTP: 0.0.0.0:$http, RPC: 0.0.0.0:$rpc)..." -ForegroundColor Green
    
    # Start in new process (visible window)
    Start-Process -FilePath $binary `
        -ArgumentList "--id $id --http-addr 0.0.0.0:$http --rpc-addr 0.0.0.0:$rpc --peers `"$peers`"" `
        -WindowStyle Normal `
        -NoNewWindow:$false
}

Write-Host "`nAll 3 nodes started! Check output windows for logs." -ForegroundColor Cyan
Write-Host "Press any key to continue..." -ForegroundColor Yellow
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
