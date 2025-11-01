# PowerShell TCP Listener on port 8000
# Usage: .\listen.ps1

$port = 8000
$listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Any, $port)
$listener.Start()

Write-Host "Listening on port $port..." -ForegroundColor Green
Write-Host "Waiting for connections... (Ctrl+C to stop)" -ForegroundColor Cyan
Write-Host ""

try {
    while ($true) {
        $client = $listener.AcceptTcpClient()
        $remoteIP = $client.Client.RemoteEndPoint.Address
        Write-Host "Connection received from: $remoteIP" -ForegroundColor Yellow
        
        $stream = $client.GetStream()
        $reader = New-Object System.IO.StreamReader($stream)
        $writer = New-Object System.IO.StreamWriter($stream)
        
        $message = $reader.ReadLine()
        if ($message) {
            Write-Host "Received: $message" -ForegroundColor Cyan
        }
        
        $writer.WriteLine("Echo: $message")
        $writer.Flush()
        
        $writer.Close()
        $stream.Close()
        $client.Close()
    }
}
catch {
    Write-Host "Error: $_" -ForegroundColor Red
}
finally {
    $listener.Stop()
    Write-Host "Listener stopped." -ForegroundColor Red
}
