import nacl from 'tweetnacl';
import { encodeBase64, decodeBase64 } from 'tweetnacl-util';

// Storage key for keypair
const KEYPAIR_STORAGE_KEY = 'p2p-nitro-keypair';

export interface KeyPair {
    publicKey: string;  // base64
    secretKey: string;  // base64
}

export interface EncryptedMessage {
    ciphertext: string;  // base64
    nonce: string;       // base64
}

/**
 * Generate a new X25519 keypair for E2EE
 */
export function generateKeyPair(): KeyPair {
    const keypair = nacl.box.keyPair();
    return {
        publicKey: encodeBase64(keypair.publicKey),
        secretKey: encodeBase64(keypair.secretKey),
    };
}

/**
 * Get or create keypair from localStorage
 */
export function getOrCreateKeyPair(): KeyPair {
    const stored = localStorage.getItem(KEYPAIR_STORAGE_KEY);
    if (stored) {
        try {
            return JSON.parse(stored) as KeyPair;
        } catch {
            // Corrupted, regenerate
        }
    }

    const keypair = generateKeyPair();
    localStorage.setItem(KEYPAIR_STORAGE_KEY, JSON.stringify(keypair));
    console.log('[Crypto] Generated new keypair');
    return keypair;
}

/**
 * Derive shared secret from our private key and their public key
 */
export function deriveSharedSecret(mySecretKey: string, theirPublicKey: string): Uint8Array {
    const mySecret = decodeBase64(mySecretKey);
    const theirPublic = decodeBase64(theirPublicKey);

    // nacl.box.before computes the shared key
    const sharedKey = nacl.box.before(theirPublic, mySecret);
    return sharedKey;
}

/**
 * Encrypt a message using XSalsa20-Poly1305 (tweetnacl's secretbox)
 */
export function encryptMessage(message: string, sharedSecret: Uint8Array): EncryptedMessage {
    // Generate random nonce (24 bytes for XSalsa20)
    const nonce = nacl.randomBytes(nacl.box.nonceLength);

    // Convert message to Uint8Array
    const messageBytes = new TextEncoder().encode(message);

    // Encrypt using box.after (uses precomputed shared key)
    const ciphertext = nacl.box.after(messageBytes, nonce, sharedSecret);

    return {
        ciphertext: encodeBase64(ciphertext),
        nonce: encodeBase64(nonce),
    };
}

/**
 * Decrypt a message
 */
export function decryptMessage(
    ciphertext: string,
    nonce: string,
    sharedSecret: Uint8Array
): string | null {
    try {
        const ciphertextBytes = decodeBase64(ciphertext);
        const nonceBytes = decodeBase64(nonce);

        // Decrypt using box.open.after (uses precomputed shared key)
        const decrypted = nacl.box.open.after(ciphertextBytes, nonceBytes, sharedSecret);

        if (!decrypted) {
            console.error('[Crypto] Decryption failed - authentication error');
            return null;
        }

        return new TextDecoder().decode(decrypted);
    } catch (e) {
        console.error('[Crypto] Decryption error:', e);
        return null;
    }
}

/**
 * Check if a message appears to be encrypted (has both ciphertext format and nonce)
 */
export function isEncryptedContent(content: string, nonce: string | null): boolean {
    return nonce !== null && nonce.length > 0;
}

// Cache for shared secrets (friendId -> sharedSecret)
const sharedSecretCache = new Map<string, Uint8Array>();

/**
 * Get or compute shared secret for a friend
 */
export function getSharedSecret(friendId: string, mySecretKey: string, friendPublicKey: string): Uint8Array {
    const cached = sharedSecretCache.get(friendId);
    if (cached) {
        return cached;
    }

    const shared = deriveSharedSecret(mySecretKey, friendPublicKey);
    sharedSecretCache.set(friendId, shared);
    console.log(`[Crypto] Computed shared secret for friend ${friendId}`);
    return shared;
}

/**
 * Clear shared secret cache (call on logout)
 */
export function clearSecretCache(): void {
    sharedSecretCache.clear();
}
