import { openDB, DBSchema, IDBPDatabase } from 'idb';
import type { KeyPair } from './crypto';

/**
 * Database schema for the ICN Wallet
 */
interface ICNWalletDB extends DBSchema {
  // Store for keypairs, indexed by DID
  keypairs: {
    key: string;
    value: {
      did: string;
      privateKey: Uint8Array;
      publicKey: Uint8Array;
      name?: string;
      createdAt: number;
      context?: string; // Federation ID, coop ID, or community ID
    };
    indexes: {
      'by-context': string;
    };
  };
  
  // Store for verifiable credentials
  credentials: {
    key: string; // UUID
    value: {
      id: string;
      type: string[];
      issuer: string;
      subject: string;
      issuanceDate: string;
      expirationDate?: string;
      context: string; // Federation ID, coop ID, or community ID
      data: any;
      proof: {
        type: string;
        created: string;
        verificationMethod: string;
        proofPurpose: string;
        proofValue: string;
      };
    };
    indexes: {
      'by-issuer': string;
      'by-subject': string;
      'by-context': string;
      'by-type': string;
    };
  };
  
  // Store for settings
  settings: {
    key: string;
    value: any;
  };
}

const DB_NAME = 'icn-wallet';
const DB_VERSION = 1;

/**
 * Initialize and get reference to the storage database
 */
export async function getDb(): Promise<IDBPDatabase<ICNWalletDB>> {
  return openDB<ICNWalletDB>(DB_NAME, DB_VERSION, {
    upgrade(db) {
      // Create keypairs store
      if (!db.objectStoreNames.contains('keypairs')) {
        const keypairStore = db.createObjectStore('keypairs', { keyPath: 'did' });
        keypairStore.createIndex('by-context', 'context');
      }
      
      // Create credentials store
      if (!db.objectStoreNames.contains('credentials')) {
        const credentialStore = db.createObjectStore('credentials', { keyPath: 'id' });
        credentialStore.createIndex('by-issuer', 'issuer');
        credentialStore.createIndex('by-subject', 'subject');
        credentialStore.createIndex('by-context', 'context');
        credentialStore.createIndex('by-type', 'type');
      }
      
      // Create settings store
      if (!db.objectStoreNames.contains('settings')) {
        db.createObjectStore('settings', { keyPath: 'key' });
      }
    }
  });
}

/**
 * Store a keypair in the database
 */
export async function storeKeypair(
  keypair: KeyPair, 
  name?: string, 
  context?: string
): Promise<string> {
  const db = await getDb();
  
  await db.put('keypairs', {
    did: keypair.did,
    privateKey: keypair.privateKey,
    publicKey: keypair.publicKey,
    name,
    context,
    createdAt: Date.now(),
  });
  
  return keypair.did;
}

/**
 * Get a keypair by DID
 */
export async function getKeypair(did: string): Promise<KeyPair | undefined> {
  const db = await getDb();
  const keypair = await db.get('keypairs', did);
  
  if (!keypair) {
    return undefined;
  }
  
  return {
    did: keypair.did,
    privateKey: keypair.privateKey,
    publicKey: keypair.publicKey,
  };
}

/**
 * Get all keypairs, optionally filtered by context
 */
export async function getAllKeypairs(context?: string): Promise<KeyPair[]> {
  const db = await getDb();
  
  let keypairs;
  if (context) {
    keypairs = await db.getAllFromIndex('keypairs', 'by-context', context);
  } else {
    keypairs = await db.getAll('keypairs');
  }
  
  return keypairs.map(k => ({
    did: k.did,
    privateKey: k.privateKey,
    publicKey: k.publicKey,
  }));
}

/**
 * Delete a keypair by DID
 */
export async function deleteKeypair(did: string): Promise<void> {
  const db = await getDb();
  await db.delete('keypairs', did);
}

/**
 * Store a verifiable credential
 */
export async function storeCredential(credential: any): Promise<string> {
  const db = await getDb();
  await db.put('credentials', credential);
  return credential.id;
}

/**
 * Get a credential by ID
 */
export async function getCredential(id: string): Promise<any | undefined> {
  const db = await getDb();
  return db.get('credentials', id);
}

/**
 * Get all credentials, optionally filtered by subject DID
 */
export async function getCredentialsBySubject(subject: string): Promise<any[]> {
  const db = await getDb();
  return db.getAllFromIndex('credentials', 'by-subject', subject);
}

/**
 * Get all credentials for a specific context
 */
export async function getCredentialsByContext(context: string): Promise<any[]> {
  const db = await getDb();
  return db.getAllFromIndex('credentials', 'by-context', context);
}

/**
 * Delete a credential by ID
 */
export async function deleteCredential(id: string): Promise<void> {
  const db = await getDb();
  await db.delete('credentials', id);
}

/**
 * Get or set a setting
 */
export async function getSetting(key: string): Promise<any> {
  const db = await getDb();
  const setting = await db.get('settings', key);
  return setting?.value;
}

/**
 * Set a setting
 */
export async function setSetting(key: string, value: any): Promise<void> {
  const db = await getDb();
  await db.put('settings', { key, value });
}

/**
 * Clear all data (for development/testing)
 */
export async function clearAllData(): Promise<void> {
  const db = await getDb();
  await db.clear('keypairs');
  await db.clear('credentials');
  await db.clear('settings');
} 