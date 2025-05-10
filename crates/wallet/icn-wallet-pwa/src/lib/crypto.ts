import * as ed from '@noble/ed25519';
import { base64url } from '@noble/ed25519/utils';

/**
 * Implements cryptographic functions for the ICN Wallet
 * 
 * Uses ed25519 for key generation and signatures, following ICN's
 * identity model based on did:key identifiers.
 */

export interface KeyPair {
  publicKey: Uint8Array;
  privateKey: Uint8Array;
  did: string;
}

/**
 * Generate a new Ed25519 keypair
 */
export async function generateKeyPair(): Promise<KeyPair> {
  const privateKey = ed.utils.randomPrivateKey();
  const publicKey = await ed.getPublicKey(privateKey);
  
  // Generate DID using the did:key method
  const did = publicKeyToDid(publicKey);
  
  return {
    publicKey,
    privateKey,
    did
  };
}

/**
 * Convert a public key to a did:key identifier
 * 
 * Based on the multicodec encoding used in the ICN codebase
 */
export function publicKeyToDid(publicKey: Uint8Array): string {
  // Prefix with the Ed25519 multicodec identifier (0xed01)
  const prefixed = new Uint8Array(2 + publicKey.length);
  prefixed[0] = 0xed;
  prefixed[1] = 0x01;
  prefixed.set(publicKey, 2);
  
  // Base64url encode and create the did:key identifier
  const encoded = base64url(prefixed);
  return `did:key:z${encoded}`;
}

/**
 * Extract a public key from a did:key identifier
 */
export function didToPublicKey(did: string): Uint8Array | null {
  if (!did.startsWith('did:key:z')) {
    return null;
  }
  
  try {
    // Extract the base64url-encoded part
    const encoded = did.slice(10); // Remove 'did:key:z'
    
    // Decode the multibase encoded value
    const decoded = base64url.decode(encoded);
    
    // Check proper multicodec prefix
    if (decoded.length < 34 || decoded[0] !== 0xed || decoded[1] !== 0x01) {
      return null;
    }
    
    // Extract the actual public key (removing the multicodec prefix)
    return decoded.slice(2);
  } catch (error) {
    console.error('Error decoding DID:', error);
    return null;
  }
}

/**
 * Sign a message with a private key
 */
export async function sign(message: Uint8Array, privateKey: Uint8Array): Promise<Uint8Array> {
  return ed.sign(message, privateKey);
}

/**
 * Verify a signature for a message with a public key
 */
export async function verify(
  signature: Uint8Array, 
  message: Uint8Array, 
  publicKey: Uint8Array
): Promise<boolean> {
  return ed.verify(signature, message, publicKey);
}

/**
 * Verify a signature from a DID
 */
export async function verifyFromDid(
  signature: Uint8Array,
  message: Uint8Array,
  did: string
): Promise<boolean> {
  const publicKey = didToPublicKey(did);
  if (!publicKey) {
    return false;
  }
  
  return verify(signature, message, publicKey);
} 