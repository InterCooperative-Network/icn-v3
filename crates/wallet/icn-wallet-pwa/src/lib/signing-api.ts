import * as crypto from './crypto';
import * as storage from './storage';

/**
 * ICN Wallet Signing API
 * 
 * Provides a secure bridge between ICN frontends (dashboard, AgoraNet) and the wallet
 * for requesting signatures and identity data via postMessage.
 * 
 * Security features:
 * - Origin validation
 * - Scoped operations (federation/coop/community)
 * - User confirmation for sensitive operations
 * - Audit logging
 */

// Define message types
export type SigningRequest = {
  id: string;          // Unique request ID for matching responses
  action: string;      // Action to perform: sign, getDid, getScopes
  origin: string;      // Origin of the request
  params: {
    scope?: {          // Organizational scope context
      federation?: string;
      coop?: string;
      community?: string;
    };
    did?: string;      // DID to use (if not provided, use active)
    payload?: any;     // Data to sign
    message?: string | Uint8Array | number[]; // Message to sign
  };
  requireConfirmation?: boolean; // Whether to confirm with user
};

export type SigningResponse = {
  id: string;          // Same ID as request
  success: boolean;    // Whether the operation succeeded
  action: string;      // Echo of the requested action
  result?: any;        // Response data
  error?: string;      // Error message if success is false
};

// Log levels
export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3
}

export class SigningAPI {
  private trustedOrigins: Set<string>;
  private activeRequests: Map<string, SigningRequest>;
  private logLevel: LogLevel;
  private activeDid: string | null = null;
  private userConfirmationCallback: ((req: SigningRequest) => Promise<boolean>) | null = null;
  
  constructor(options: {
    trustedOrigins?: string[];
    logLevel?: LogLevel;
    confirmationCallback?: (req: SigningRequest) => Promise<boolean>;
  } = {}) {
    this.trustedOrigins = new Set(options.trustedOrigins || [window.location.origin]);
    this.activeRequests = new Map();
    this.logLevel = options.logLevel || LogLevel.INFO;
    this.userConfirmationCallback = options.confirmationCallback || null;
    
    // Automatically initialize listeners
    this.initMessageListener();
    this.loadActiveDid();
    
    this.log(LogLevel.INFO, "ICN Wallet Signing API initialized");
  }
  
  /**
   * Initialize the message listener for incoming signing requests
   */
  private initMessageListener(): void {
    window.addEventListener('message', async (event) => {
      try {
        // Validate the origin
        if (!this.isTrustedOrigin(event.origin)) {
          this.log(LogLevel.WARN, `Ignoring message from untrusted origin: ${event.origin}`);
          return;
        }
        
        const request = event.data as SigningRequest;
        
        // Validate request structure
        if (!this.isValidRequest(request)) {
          this.log(LogLevel.WARN, `Invalid request format`, request);
          return;
        }
        
        // Add origin information explicitly
        request.origin = event.origin;
        
        // Store the request
        this.activeRequests.set(request.id, request);
        
        // Process the request and get response
        const response = await this.processRequest(request);
        
        // Send the response back
        if (event.source && 'postMessage' in event.source) {
          (event.source as WindowProxy).postMessage(response, { targetOrigin: event.origin });
          this.log(LogLevel.DEBUG, `Response sent to ${event.origin}`, response);
        }
      } catch (error) {
        this.log(LogLevel.ERROR, `Error processing message: ${error}`);
      }
    });
  }
  
  /**
   * Check if an origin is in the trusted origins list
   */
  private isTrustedOrigin(origin: string): boolean {
    return this.trustedOrigins.has(origin);
  }
  
  /**
   * Add a trusted origin
   */
  public addTrustedOrigin(origin: string): void {
    this.trustedOrigins.add(origin);
    this.log(LogLevel.INFO, `Added trusted origin: ${origin}`);
  }
  
  /**
   * Remove a trusted origin
   */
  public removeTrustedOrigin(origin: string): void {
    this.trustedOrigins.delete(origin);
    this.log(LogLevel.INFO, `Removed trusted origin: ${origin}`);
  }
  
  /**
   * Set the user confirmation callback
   */
  public setUserConfirmationCallback(callback: (req: SigningRequest) => Promise<boolean>): void {
    this.userConfirmationCallback = callback;
  }
  
  /**
   * Validate the structure of a signing request
   */
  private isValidRequest(request: any): boolean {
    if (!request) return false;
    if (typeof request !== 'object') return false;
    if (!request.id || typeof request.id !== 'string') return false;
    if (!request.action || typeof request.action !== 'string') return false;
    
    return true;
  }
  
  /**
   * Process a signing request and return a response
   */
  private async processRequest(request: SigningRequest): Promise<SigningResponse> {
    this.log(LogLevel.INFO, `Processing request: ${request.action}`, request);
    
    // Check if we need user confirmation
    if (request.requireConfirmation && this.userConfirmationCallback) {
      const confirmed = await this.userConfirmationCallback(request);
      if (!confirmed) {
        return this.createResponse(request, false, null, 'User rejected the request');
      }
    }
    
    try {
      switch (request.action) {
        case 'sign':
          return await this.handleSignRequest(request);
          
        case 'getDid':
          return await this.handleGetDidRequest(request);
          
        case 'getScopes':
          return await this.handleGetScopesRequest(request);
          
        case 'trustOrigin':
          return this.handleTrustOriginRequest(request);
          
        default:
          return this.createResponse(request, false, null, `Unknown action: ${request.action}`);
      }
    } catch (error) {
      this.log(LogLevel.ERROR, `Error processing request: ${error}`);
      return this.createResponse(
        request, 
        false, 
        null, 
        error instanceof Error ? error.message : String(error)
      );
    }
  }
  
  /**
   * Handle a signature request
   */
  private async handleSignRequest(request: SigningRequest): Promise<SigningResponse> {
    const { params } = request;
    
    // Validate required parameters
    if (!params.message && !params.payload) {
      return this.createResponse(request, false, null, 'No message or payload provided');
    }
    
    // Get the DID to use for signing
    const did = params.did || this.activeDid;
    if (!did) {
      return this.createResponse(request, false, null, 'No active DID and no DID specified');
    }
    
    // Get the keypair for the DID
    const keypair = await storage.getKeypair(did);
    if (!keypair) {
      return this.createResponse(request, false, null, `DID not found: ${did}`);
    }
    
    // Check if the DID matches the requested scope
    if (params.scope) {
      // TODO: implement scope validation once we have proper scoping in the storage
    }
    
    // Choose what to sign (message or payload)
    let messageToSign: Uint8Array;
    if (params.message) {
      if (typeof params.message === 'string') {
        messageToSign = new TextEncoder().encode(params.message);
      } else if (Array.isArray(params.message)) {
        messageToSign = new Uint8Array(params.message);
      } else {
        messageToSign = params.message as Uint8Array;
      }
    } else {
      // For general payloads, stringify and encode
      const payloadStr = JSON.stringify(params.payload);
      messageToSign = new TextEncoder().encode(payloadStr);
    }
    
    // Sign the message
    const signature = await crypto.sign(messageToSign, keypair.privateKey);
    
    // Return the result
    return this.createResponse(request, true, {
      signature: Array.from(signature),
      did: did,
      scope: params.scope || null
    });
  }
  
  /**
   * Handle a request to get the active DID
   */
  private async handleGetDidRequest(request: SigningRequest): Promise<SigningResponse> {
    const { params } = request;
    
    // If a specific scope is requested, find DIDs for that scope
    if (params.scope) {
      const federationId = params.scope.federation;
      const coopId = params.scope.coop;
      const communityId = params.scope.community;
      
      let scopeQuery = '';
      if (federationId) scopeQuery = `federation:${federationId}`;
      else if (coopId) scopeQuery = `coop:${coopId}`;
      else if (communityId) scopeQuery = `community:${communityId}`;
      
      if (scopeQuery) {
        const db = await storage.getDb();
        const keypairs = await db.getAllFromIndex('keypairs', 'by-context', scopeQuery);
        
        if (keypairs.length === 0) {
          return this.createResponse(
            request, 
            false, 
            null, 
            `No identities found for scope: ${JSON.stringify(params.scope)}`
          );
        }
        
        // Return the first matching DID
        return this.createResponse(request, true, {
          did: keypairs[0].did,
          scope: params.scope
        });
      }
    }
    
    // If no scope is specified, return the active DID
    if (!this.activeDid) {
      const keypairs = await storage.getAllKeypairs();
      if (keypairs.length === 0) {
        return this.createResponse(request, false, null, 'No identities available');
      }
      this.activeDid = keypairs[0].did;
    }
    
    return this.createResponse(request, true, { 
      did: this.activeDid,
      scope: null 
    });
  }
  
  /**
   * Handle a request to get available scopes
   */
  private async handleGetScopesRequest(request: SigningRequest): Promise<SigningResponse> {
    // Get all keypairs to extract scopes
    const db = await storage.getDb();
    const keypairs = await db.getAll('keypairs');
    
    // Extract unique scopes
    const scopes = new Map<string, { 
      type: 'federation' | 'coop' | 'community', 
      id: string,
      dids: string[] 
    }>();
    
    for (const keypair of keypairs) {
      if (keypair.context) {
        const [type, id] = keypair.context.split(':');
        
        if (!type || !id) continue;
        
        const scopeKey = `${type}:${id}`;
        let scope = scopes.get(scopeKey);
        
        if (!scope) {
          scope = { 
            type: type as any, 
            id, 
            dids: [] 
          };
          scopes.set(scopeKey, scope);
        }
        
        scope.dids.push(keypair.did);
      }
    }
    
    // Convert to response format
    const result = {
      federations: [] as any[],
      coops: [] as any[],
      communities: [] as any[]
    };
    
    for (const scope of scopes.values()) {
      const item = {
        id: scope.id,
        dids: scope.dids
      };
      
      if (scope.type === 'federation') {
        result.federations.push(item);
      } else if (scope.type === 'coop') {
        result.coops.push(item);
      } else if (scope.type === 'community') {
        result.communities.push(item);
      }
    }
    
    return this.createResponse(request, true, result);
  }
  
  /**
   * Handle a request to trust an origin
   */
  private handleTrustOriginRequest(request: SigningRequest): SigningResponse {
    const { params } = request;
    
    if (!params || !params.payload || typeof params.payload !== 'string') {
      return this.createResponse(request, false, null, 'No origin provided');
    }
    
    const originToTrust = params.payload;
    
    // Only allow origins added by the wallet itself
    if (request.origin !== window.location.origin) {
      return this.createResponse(
        request, 
        false, 
        null, 
        'Origin trust can only be managed by the wallet itself'
      );
    }
    
    this.addTrustedOrigin(originToTrust);
    
    return this.createResponse(request, true, { 
      origin: originToTrust,
      trusted: true 
    });
  }
  
  /**
   * Create a standardized response object
   */
  private createResponse(
    request: SigningRequest, 
    success: boolean, 
    result: any = null, 
    error: string | null = null
  ): SigningResponse {
    return {
      id: request.id,
      success,
      action: request.action,
      result,
      error: error || undefined
    };
  }
  
  /**
   * Set the active DID
   */
  public async setActiveDid(did: string): Promise<boolean> {
    // Verify the DID exists
    const keypair = await storage.getKeypair(did);
    if (!keypair) {
      this.log(LogLevel.ERROR, `Cannot set active DID: ${did} not found`);
      return false;
    }
    
    this.activeDid = did;
    
    // Store in settings
    await storage.setSetting('activeDid', did);
    
    this.log(LogLevel.INFO, `Active DID set to: ${did}`);
    return true;
  }
  
  /**
   * Load the active DID from storage
   */
  private async loadActiveDid(): Promise<void> {
    try {
      // Try to load from settings
      const storedDid = await storage.getSetting('activeDid');
      
      if (storedDid) {
        // Verify the DID exists
        const keypair = await storage.getKeypair(storedDid);
        if (keypair) {
          this.activeDid = storedDid;
          return;
        }
      }
      
      // If no active DID, try to use the first available one
      const keypairs = await storage.getAllKeypairs();
      if (keypairs.length > 0) {
        this.activeDid = keypairs[0].did;
        await storage.setSetting('activeDid', this.activeDid);
      }
    } catch (error) {
      this.log(LogLevel.ERROR, `Error loading active DID: ${error}`);
    }
  }
  
  /**
   * Internal logging function
   */
  private log(level: LogLevel, message: string, data?: any): void {
    if (level < this.logLevel) return;
    
    const timestamp = new Date().toISOString();
    const prefix = `[ICN Wallet API ${LogLevel[level]}]`;
    
    if (data) {
      console.log(`${timestamp} ${prefix} ${message}`, data);
    } else {
      console.log(`${timestamp} ${prefix} ${message}`);
    }
  }
  
  /**
   * Set the log level
   */
  public setLogLevel(level: LogLevel): void {
    this.logLevel = level;
  }
}

// Export singleton instance with default settings
export const signingApi = new SigningAPI({
  logLevel: process.env.NODE_ENV === 'development' ? LogLevel.DEBUG : LogLevel.INFO
});

/**
 * Example usage of the SigningAPI from an external application:
 * 
 * ```javascript
 * // Function to request a signature
 * async function requestSignature(message, scope) {
 *   return new Promise((resolve, reject) => {
 *     // Create a unique ID for this request
 *     const requestId = `sig-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
 *     
 *     // Create the request
 *     const request = {
 *       id: requestId,
 *       action: 'sign',
 *       params: {
 *         message: typeof message === 'string' 
 *           ? message 
 *           : Array.from(message),
 *         scope
 *       },
 *       requireConfirmation: true
 *     };
 *     
 *     // Set up response handler
 *     const responseHandler = (event) => {
 *       const response = event.data;
 *       
 *       // Check if this is a response to our request
 *       if (response && response.id === requestId) {
 *         // Clean up event listener
 *         window.removeEventListener('message', responseHandler);
 *         
 *         if (response.success) {
 *           resolve(response.result);
 *         } else {
 *           reject(new Error(response.error || 'Unknown error'));
 *         }
 *       }
 *     };
 *     
 *     // Listen for the response
 *     window.addEventListener('message', responseHandler);
 *     
 *     // Send the request to the wallet
 *     // This assumes the wallet is open in a parent window or iframe
 *     window.parent.postMessage(request, 'https://wallet.icn.example.com');
 *   });
 * }
 * 
 * // Usage
 * async function signData() {
 *   try {
 *     const message = 'Hello, ICN!';
 *     const scope = { federation: 'my-federation' };
 *     
 *     const result = await requestSignature(message, scope);
 *     console.log('Signature:', result.signature);
 *     console.log('Signed with DID:', result.did);
 *   } catch (error) {
 *     console.error('Signing error:', error);
 *   }
 * }
 * ```
 */ 