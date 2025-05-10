import * as crypto from './crypto';
import * as storage from './storage';

export type ApiRequest = {
  id: string;
  action: string;
  params: any;
};

export type ApiResponse = {
  id: string;
  success: boolean;
  data?: any;
  error?: string;
};

export type ApiCallback = (response: ApiResponse) => void;

/**
 * ICN Wallet API
 * 
 * Exposes a set of methods for interacting with the ICN Wallet
 * from external applications via postMessage or direct import.
 */
export class WalletApi {
  private origin: string;
  private trustedOrigins: Set<string>;
  
  constructor() {
    this.origin = window.location.origin;
    this.trustedOrigins = new Set([this.origin]);
    
    // Initialize message listener
    this.initMessageListener();
  }
  
  /**
   * Initialize the message listener for communication with other apps
   */
  private initMessageListener() {
    window.addEventListener('message', async (event) => {
      // Security check - verify origin
      if (!this.trustedOrigins.has(event.origin)) {
        console.warn(`Ignoring message from untrusted origin: ${event.origin}`);
        return;
      }
      
      const request = event.data as ApiRequest;
      if (!request || !request.id || !request.action) {
        return;
      }
      
      try {
        // Process the request
        const result = await this.processRequest(request);
        
        // Send the response back to the sender
        event.source?.postMessage(result, { targetOrigin: event.origin });
      } catch (error) {
        console.error('Error processing wallet API request:', error);
        
        // Send error response
        event.source?.postMessage({
          id: request.id,
          success: false,
          error: error instanceof Error ? error.message : 'Unknown error',
        }, { targetOrigin: event.origin });
      }
    });
  }
  
  /**
   * Process an API request
   */
  private async processRequest(request: ApiRequest): Promise<ApiResponse> {
    const { id, action, params } = request;
    
    try {
      let data;
      
      switch (action) {
        case 'getIdentities':
          data = await this.getIdentities(params?.context);
          break;
          
        case 'createIdentity':
          data = await this.createIdentity(params?.name, params?.context);
          break;
          
        case 'sign':
          data = await this.sign(params?.did, params?.message);
          break;
          
        case 'verify':
          data = await this.verify(params?.did, params?.message, params?.signature);
          break;
          
        case 'trustOrigin':
          data = this.trustOrigin(params?.origin);
          break;
          
        default:
          return {
            id,
            success: false,
            error: `Unknown action: ${action}`,
          };
      }
      
      return {
        id,
        success: true,
        data,
      };
    } catch (error) {
      console.error(`Error processing action '${action}':`, error);
      
      return {
        id,
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }
  
  /**
   * Get all identities, optionally filtered by context
   */
  public async getIdentities(context?: string) {
    const keypairs = await storage.getAllKeypairs(context);
    
    // Return only public information
    return keypairs.map(kp => ({
      did: kp.did,
      publicKey: Array.from(kp.publicKey),
    }));
  }
  
  /**
   * Create a new identity
   */
  public async createIdentity(name?: string, context?: string) {
    const keypair = await crypto.generateKeyPair();
    await storage.storeKeypair(keypair, name, context);
    
    return {
      did: keypair.did,
      publicKey: Array.from(keypair.publicKey),
    };
  }
  
  /**
   * Sign a message with the specified DID
   */
  public async sign(did: string, message: string | Uint8Array) {
    // Get the keypair
    const keypair = await storage.getKeypair(did);
    if (!keypair) {
      throw new Error(`Identity not found: ${did}`);
    }
    
    // Convert message to Uint8Array if it's a string
    const messageBytes = typeof message === 'string' 
      ? new TextEncoder().encode(message)
      : message;
      
    // Sign the message
    const signature = await crypto.sign(messageBytes, keypair.privateKey);
    
    return {
      signature: Array.from(signature),
    };
  }
  
  /**
   * Verify a signature
   */
  public async verify(
    did: string, 
    message: string | Uint8Array, 
    signature: number[] | Uint8Array
  ) {
    // Convert message to Uint8Array if it's a string
    const messageBytes = typeof message === 'string' 
      ? new TextEncoder().encode(message)
      : message;
      
    // Convert signature to Uint8Array if it's an array
    const signatureBytes = Array.isArray(signature)
      ? new Uint8Array(signature)
      : signature;
      
    // Verify the signature
    const isValid = await crypto.verifyFromDid(signatureBytes, messageBytes, did);
    
    return { isValid };
  }
  
  /**
   * Trust an origin for communication
   */
  public trustOrigin(origin: string) {
    if (!origin || typeof origin !== 'string') {
      throw new Error('Invalid origin');
    }
    
    this.trustedOrigins.add(origin);
    return { trusted: true, origin };
  }
}

// Export singleton instance
export const walletApi = new WalletApi(); 