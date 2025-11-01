# Encryption Key Implementation Guide

## Overview

The system has been updated to return the auto-generated encryption key to users after encryption, and require it for decryption. This ensures users always have the key needed to decrypt their images.

## Changes Made

### Backend (Rust - `src/api.rs`)

#### 1. **Modified `embed_image_into_cover` function signature**
   - **Before**: `Result<Vec<u8>>` - returned only stego image bytes
   - **After**: `Result<(Vec<u8>, String)>` - returns tuple of (stego_image_bytes, key_in_hex)

```rust
fn embed_image_into_cover(secret_bytes: &[u8]) 
    -> Result<(Vec<u8>, String), Box<dyn std::error::Error + Send + Sync>>
```

#### 2. **Added Base64 encoding helper**
   - Created `base64_encode()` function to encode binary stego images as base64 strings
   - Allows images to be embedded in JSON responses

#### 3. **Updated `/image/steg` endpoint responses**
   Both follower (forwarded) and leader (local) paths now:
   - Parse the result tuple: `(stego_bytes, key_hex)`
   - Encode the stego image as base64 with data URI prefix
   - Return JSON response with both key and image:
   ```json
   {
     "key": "hex-encoded-encryption-key",
     "image": "data:image/png;base64,..."
   }
   ```
   - Log the encryption key for debugging: `🔑 Encryption key: <hex>`

#### 4. **Added hex dependency to `Cargo.toml`**
   ```toml
   hex = "0.4"
   ```

### API Client (TypeScript - `src/api/client.ts`)

#### 1. **Updated `uploadImageForEncryption` response type**
   - Now includes `key?: string` field
   - Parses JSON response instead of binary blob
   - Extracts both `key` and base64-encoded `image`
   - Converts base64 image back to Blob for consistency

```typescript
async uploadImageForEncryption(
  nodeId: number,
  imageFile: File
): Promise<{ 
  success: boolean; 
  data?: Blob; 
  key?: string;  // Added
  error?: string; 
  latency: number; 
  processedBy?: number 
}>
```

#### 2. **Updated `decryptImageFromCluster` to accept encryption key**
   - Added required `encryptionKey: string` parameter
   - Passes key as query parameter: `/image/decrypt?key=<hex-key>`
   - URL encodes the key for safety

```typescript
async decryptImageFromCluster(
  stegoFile: File,
  encryptionKey: string  // Added parameter
): Promise<{ ... }>
```

### UI Component (React - `src/components/ImageEncryptionTab.tsx`)

#### 1. **Added encryption key state management**
   ```typescript
   const [manualEncryptionKey, setManualEncryptionKey] = useState<string>('');
   ```

#### 2. **Updated `handleEncrypt` function**
   - Now automatically stores the returned encryption key
   - Populates the `manualEncryptionKey` state with the key from the server

#### 3. **Updated `handleDecrypt` function**
   - Validates that encryption key is provided
   - Passes `manualEncryptionKey` to API call
   - Shows error if key is missing

#### 4. **Added encryption key display section**
   After encryption completes, shows:
   - Yellow highlighted box with the encryption key
   - Copy button to copy key to clipboard
   - Clear instruction: "🔑 Encryption Key (Save this!)"

#### 5. **Added encryption key input section**
   Before decryption, shows:
   - Textarea to paste/edit the encryption key
   - Auto-populated if you just encrypted an image
   - Blue highlighted box with instructions
   - Decryption button disabled until key is provided

#### 6. **Added visual feedback**
   - Loading indicators during encryption/decryption
   - Key display with copy functionality
   - Required field validation

## User Workflow

### Encryption Flow
1. User selects an image to encrypt
2. Clicks "Encrypt & Get Steganography Image"
3. Server generates a random 32-byte encryption key using ChaCha20Poly1305
4. Server encrypts the image with the key
5. Server embeds encrypted data into a cover image (LSB steganography)
6. **Server returns both:**
   - The stego image (PNG)
   - The encryption key (hex string)
7. UI displays:
   - Stego image that user can download
   - Encryption key in a highlighted box with copy button
8. User saves both the stego image and the key

### Decryption Flow
1. User obtains the stego image and encryption key (from previous encryption)
2. User enters/pastes the encryption key in the "Encryption Key" textarea
3. User selects the stego image file
4. Server extracts the encrypted data from the stego image (LSB extraction)
5. Server decrypts the data using the provided key
6. Server returns the original hidden image
7. UI displays the extracted image for download

## Key Exchange Flow

```
┌──────────────┐                          ┌──────────────┐
│   User       │                          │   Server     │
│   (Client)   │                          │   (Cluster)  │
└──────────────┘                          └──────────────┘
       │                                         │
       │  1. POST image to /image/steg          │
       │──────────────────────────────────────>│
       │                                         │
       │  2. Generate random key                │
       │     Encrypt image                      │
       │     Embed in cover                     │
       │                                         │
       │  3. JSON Response:                     │
       │     { key: "hex...", image: "..." }   │
       │<──────────────────────────────────────│
       │                                         │
       │  4. Display key with copy button       │
       │     Download stego image               │
       │                                         │
       │  ... Later ...                         │
       │                                         │
       │  5. POST image + key to /image/decrypt │
       │     ?key=hex...                        │
       │──────────────────────────────────────>│
       │                                         │
       │  6. Extract from stego                 │
       │     Decrypt with key                   │
       │                                         │
       │  7. Binary Response:                   │
       │     Original hidden image              │
       │<──────────────────────────────────────│
       │                                         │
       │  8. Download extracted image           │
       │                                         │
```

## Technical Details

### Encryption Key Format
- **Type**: 32-byte array (256-bit)
- **Generation**: `OsRng` (cryptographically secure random)
- **Encryption**: ChaCha20Poly1305 AEAD cipher
- **Nonce**: 12-byte random value (generated per encryption)
- **Serialization**: Hex string (64 characters)
- **Example**: `a1b2c3d4e5f6...` (64 hex digits)

### Payload Structure (in stego image)
```
[4 bytes]  : Length of ciphertext (little-endian u32)
[12 bytes] : Nonce for decryption
[N bytes]  : Ciphertext (image data encrypted with key)
```

### Response Format
```json
{
  "key": "a1b2c3d4e5f6g7h8...",
  "image": "data:image/png;base64,iVBORw0KGgoAAAANS..."
}
```

## Error Handling

### Encryption Errors
- Image too large (>10MB) → HTTP 400
- Encryption failure → HTTP 500
- Node not available → HTTP 503 (then retry to other node)

### Decryption Errors
- Missing key parameter → HTTP 400 (requires `?key=...`)
- Invalid key format → HTTP 500 (decryption fails)
- Corrupted stego image → HTTP 500 (extraction fails)
- Image too large → HTTP 400

### UI Validation
- Encryption: Requires image file selected
- Decryption: Requires both image file AND encryption key
- Buttons disabled until requirements met

## Testing the Implementation

### Test 1: Encrypt and Decrypt Same Image
```bash
1. Open UI at http://localhost:5173
2. Select test image
3. Click "Encrypt & Get Steganography Image"
4. Note the encryption key displayed
5. Download the stego image
6. Paste the key in "Encryption Key" textarea
7. Upload the stego image
8. Verify decrypted image matches original
```

### Test 2: Cross-Node Processing
```bash
1. Encrypt image (processes on Node 1)
2. Decrypt with key (processes on Node 2 or 3)
3. Verify load balancing is working
4. Check server logs for node selections
```

### Test 3: Key Validation
```bash
1. Encrypt image
2. Modify the key slightly
3. Try to decrypt with wrong key
4. Verify decryption fails with error
```

## Backward Compatibility

⚠️ **Breaking Change**: The `/image/steg` endpoint response format has changed
- **Old format**: Binary PNG blob
- **New format**: JSON with `{ key, image }`

Clients must be updated to parse JSON response.

## Security Considerations

✅ **What's Secure**
- Keys generated with `OsRng` (cryptographically secure)
- ChaCha20Poly1305 is authenticated encryption
- Keys transmitted over HTTPS (in production)
- No key is stored on server

⚠️ **What to be Aware Of**
- Key is displayed in plain text in UI (user's responsibility to protect)
- Key should be transmitted securely (use HTTPS, encrypted channels)
- Nonce is unique per encryption (automatically generated)
- Users must securely store keys to decrypt later

🔐 **Recommendations**
1. Always use HTTPS in production
2. Store encryption keys securely on client side
3. Don't share keys over unencrypted channels
4. Use separate keys for different images
5. Implement key rotation policies if needed

## Deployment Notes

### Build Backend
```bash
cd raft-openraft-demo
cargo build --release
```

### Run Servers
```bash
# Terminal 1: Node 1
cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001

# Terminal 2: Node 2
cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002

# Terminal 3: Node 3
cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003
```

### Build & Run UI
```bash
cd raft-client-ui
npm install
npm run dev
# Open http://localhost:5173
```

## Conclusion

This implementation ensures that:
- ✅ Users always receive the encryption key with encrypted images
- ✅ Keys are auto-generated securely
- ✅ UI makes key management simple and intuitive
- ✅ Decryption requires the original key (prevents unauthorized decryption)
- ✅ System works with load balancing (any node can process)
- ✅ Full end-to-end encryption and decryption workflow
