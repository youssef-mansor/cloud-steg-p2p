# Decryption Troubleshooting Guide

## Common Issues and Solutions

### Issue: "Failed to fetch" Error

#### Possible Causes & Solutions

##### 1. **Server Nodes Not Running**
```bash
# Check if servers are running
netstat -ano | findstr :8001
netstat -ano | findstr :8002
netstat -ano | findstr :8003
```

✅ **Solution**: Start the servers
```bash
# Terminal 1
cd raft-openraft-demo
cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"

# Terminal 2
cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"

# Terminal 3
cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"
```

Wait for: `🌐 HTTP API listening on 0.0.0.0:8001` (and same for 8002, 8003)

---

##### 2. **Incorrect Encryption Key**

The key must be exactly as provided after encryption:
- ❌ DO NOT modify the key
- ❌ DO NOT add spaces or extra characters
- ✅ Copy the key directly from the "Encryption Key" box using the Copy button

**Check key format**:
- Key should be 64 hexadecimal characters (0-9, a-f)
- Example: `a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6`

---

##### 3. **Wrong Stego Image**

Make sure you're uploading the correct stego image:
- ✅ Should be the PNG file downloaded after encryption
- ✅ Should be named something like `encrypted-image.png`
- ❌ Should NOT be the original image
- ❌ Should NOT be a different encrypted image

---

##### 4. **Browser Console Error Details**

Open browser developer console to see detailed error messages:

**Chrome/Edge**: Press `F12` → Click "Console" tab
**Firefox**: Press `F12` → Click "Console" tab

Look for messages like:
```
🔓 Starting decryption with key length: 64
Attempting decryption on node 1 (http://localhost:8001/image/decrypt?key=...)
✅ Node 1 succeeded: ...
```

Or errors like:
```
❌ Node 1: HTTP 500
❌ Node 2: HTTP 503
❌ Node 3: HTTP 500
```

---

### Issue: HTTP 500 Error (Server Error)

#### Possible Causes

1. **Invalid Key Format**
   - The key is not properly hexadecimal
   - The key is incomplete or corrupted

2. **Stego Image Corrupted**
   - The image file was modified after encryption
   - The image upload was incomplete

3. **Encryption/Decryption Mismatch**
   - Used a different key than what encrypted the image
   - Image was encrypted with different algorithm settings

#### Solution

Check server logs for detailed error messages:
```
❌ Node X failed to extract image: Failed to decrypt: tag verification failed
```

---

### Issue: HTTP 503 Error (Service Unavailable)

#### Possible Causes

1. **Follower Node Received Direct Request**
   - The node is a follower, not the leader
   - The leader is not running

#### Solution

- Make sure the leader node (Node 1) is running
- The client automatically retries other nodes
- If all return 503, wait a few seconds for leader election and retry

---

### Issue: HTTP 400 Error (Bad Request)

#### Possible Causes

1. **Key Parameter Missing or Malformed**
   - The `?key=` parameter was not included
   - The key contains invalid characters

2. **Image Size Exceeds 10MB**
   - The stego image is too large
   - This is a safety limit to prevent memory exhaustion

#### Solution

- Ensure you're using the correct key format
- Ensure the stego image is less than 10MB
- Try encrypting a smaller image

---

## Detailed Decryption Flow

### What Should Happen

```
1. User enters encryption key in textarea
2. User selects stego image file
3. Browser sends POST request to all 3 servers:
   POST /image/decrypt?key=<hex-key>
   Content-Type: application/octet-stream
   Body: <stego-image-binary>

4. Each server responds with either:
   - 200 OK with extracted image (binary)
   - 503 if follower and no x-raft-forwarded header
   - 500 if decryption failed

5. Browser accepts first 200 response
6. Browser displays extracted image
```

### Expected Console Logs

#### Success Case
```
🔓 Starting decryption with key length: 64
Attempting decryption on node 1 ...
✅ Node 1 succeeded: processed by node 1, received 12345 bytes, latency: 123ms
✅ Decryption succeeded on node 1, latency: 456ms
```

#### Partial Failure (Falls Back to Other Node)
```
🔓 Starting decryption with key length: 64
Attempting decryption on node 1 ...
❌ Node 1 returned HTTP 503
Attempting decryption on node 2 ...
✅ Node 2 succeeded: ...
✅ Decryption succeeded on node 2, latency: 456ms
```

#### Complete Failure
```
🔓 Starting decryption with key length: 64
Attempting decryption on node 1 ...
❌ Node 1 returned HTTP 500
Attempting decryption on node 2 ...
❌ Node 2 returned HTTP 503
Attempting decryption on node 3 ...
❌ Node 3 returned HTTP 500
❌ All nodes failed: Node 1: HTTP 500; Node 2: HTTP 503; Node 3: HTTP 500
❌ Decryption failed: All nodes failed: ...
```

---

## Network Debugging

### Check Server Connectivity

```bash
# From PowerShell
Test-NetConnection localhost -Port 8001
Test-NetConnection localhost -Port 8002
Test-NetConnection localhost -Port 8003
```

Expected output:
```
ComputerName     : localhost
RemoteAddress    : 127.0.0.1
RemotePort       : 8001
InterfaceAlias   : Ethernet
SourceAddress    : 127.0.0.1
TcpTestSucceeded : True
```

### Test Server Endpoint Manually

```bash
# Using curl (if available)
# First get a stego image, then decrypt it

# Decrypt endpoint test with dummy key
curl -X POST \
  "http://localhost:8001/image/decrypt?key=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6" \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@stego-image.png" \
  -v
```

Expected responses:
- **200**: Success (returns binary image data)
- **400**: Bad request (invalid key)
- **500**: Server error (decryption failed)
- **503**: Node is follower (not leader)

---

## Step-by-Step Verification

### 1. Verify Encryption Works
```
✅ Select an image
✅ Click "Encrypt & Get Steganography Image"
✅ Key appears in yellow box
✅ Can download stego image
```

### 2. Verify Key is Correct Format
```
✅ Key is 64 hexadecimal characters (0-9, a-f)
✅ No spaces or special characters
✅ Copy key with Copy button (don't paste manually)
```

### 3. Verify Stego Image is Correct
```
✅ Stego image is the downloaded PNG
✅ File size is similar to original image size + ~100 bytes
✅ Image can be opened in image viewer
```

### 4. Verify Server is Running
```
✅ Terminal shows: "🌐 HTTP API listening on 0.0.0.0:8001"
✅ All 3 nodes should be running
✅ At least Node 1 should be leader
```

### 5. Verify Decryption
```
✅ Paste key in textarea
✅ Select stego image
✅ Click "Select Stego Image to Decrypt"
✅ Wait for decryption to complete
✅ Extracted image appears
```

---

## Common Mistakes

### ❌ Mistake 1: Using Original Image for Decryption
- ❌ Uploading the original image instead of stego image
- ✅ Solution: Use the downloaded `encrypted-image.png`

### ❌ Mistake 2: Modifying the Encryption Key
- ❌ Manually editing the key
- ❌ Adding/removing characters
- ✅ Solution: Copy the exact key using Copy button

### ❌ Mistake 3: Server Not Running
- ❌ No terminal with running servers
- ❌ Only 1 or 2 servers running instead of 3
- ✅ Solution: Start all 3 server nodes

### ❌ Mistake 4: UI Not Updated
- ❌ Using old UI without the encryption key feature
- ❌ Decryption field missing
- ✅ Solution: Rebuild UI with `npm run dev`

### ❌ Mistake 5: Wrong Node URL
- ❌ UI configured for wrong node addresses
- ❌ Servers running on different ports than expected
- ✅ Solution: Check `src/api/client.ts` for correct addresses

---

## Quick Checklist

Before each decryption attempt, verify:

- [ ] All 3 servers are running (`cargo run --release --`)
- [ ] Leader node elected (check logs for leader election)
- [ ] Encryption key is 64 hex characters
- [ ] Using the correct stego image (encrypted output, not original)
- [ ] Browser console open to see detailed logs
- [ ] No typos in encryption key
- [ ] Internet connection is stable
- [ ] Firewall not blocking ports 8001-8003

---

## Getting Help

If you're still stuck, collect this information:

1. **Browser Console Output** (press F12, copy all error messages)
2. **Server Log Output** (copy relevant error messages from terminals)
3. **Steps to Reproduce** (what did you do before the error?)
4. **Encryption Key Format** (does it look like hex? is it 64 chars?)
5. **Which Node Failed** (is it Node 1, 2, or all?)

Then check:
- ENCRYPTION_KEY_IMPLEMENTATION.md for API details
- README.md for architecture overview
- Server logs for detailed decryption errors
