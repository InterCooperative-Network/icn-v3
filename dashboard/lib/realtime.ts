import { io, Socket } from 'socket.io-client';
import { useState, useEffect } from 'react';
import { ExecutionReceipt, Token, TokenTransaction } from './api';

// Configuration with federation scoping
export interface WebSocketConfig {
  federationId?: string;  // Optional federation scope
  authToken?: string;     // JWT token for auth
}

// Event types for the real-time updates
export enum RealtimeEvent {
  RECEIPT_CREATED = 'receipt:created',
  TOKEN_TRANSFERRED = 'token:transferred',
  TOKEN_MINTED = 'token:minted',
  TOKEN_BURNED = 'token:burned',
  FEDERATION_NODE_STATUS = 'federation:node:status'
}

// Class to handle WebSocket connection and real-time updates
class RealtimeService {
  private connections: Map<string, Socket> = new Map();
  private listeners: Map<string, Map<string, Set<Function>>> = new Map();
  private lastUpdated: Map<string, Date> = new Map();
  
  // Connect to a specific federation namespace
  connect(config: WebSocketConfig = {}) {
    const { federationId = 'global', authToken } = config;
    
    // Create a unique connection key
    const connectionKey = `federation:${federationId}`;
    
    // Return if already connected
    if (this.connections.has(connectionKey)) return;
    
    // Create socket URL - add federation as namespace
    const SOCKET_URL = process.env.NEXT_PUBLIC_SOCKET_URL || 'http://localhost:8081';
    const namespaceUrl = `${SOCKET_URL}/${federationId}`;
    
    // Configure socket with auth if token exists
    const socketOptions = {
      reconnectionAttempts: 5,
      reconnectionDelay: 1000,
      transports: ['websocket'],
      auth: authToken ? { token: authToken } : undefined
    };
    
    // Create new socket connection
    const socket = io(namespaceUrl, socketOptions);
    
    // Set up event handlers
    socket.on('connect', () => {
      console.log(`Connected to federation: ${federationId}`);
    });
    
    socket.on('disconnect', () => {
      console.log(`Disconnected from federation: ${federationId}`);
    });
    
    socket.on('connect_error', (err) => {
      console.warn(`Federation connection error (${federationId}):`, err);
    });
    
    // Subscribe to all events for this federation
    Object.values(RealtimeEvent).forEach(eventType => {
      socket.on(eventType, (data) => {
        this.lastUpdated.set(`${connectionKey}:${eventType}`, new Date());
        
        // Call all event listeners for this federation
        if (!this.listeners.has(connectionKey)) return;
        const federationListeners = this.listeners.get(connectionKey)!;
        
        if (!federationListeners.has(eventType)) return;
        federationListeners.get(eventType)!.forEach(callback => callback(data));
      });
    });
    
    // Store the connection
    this.connections.set(connectionKey, socket);
  }
  
  // Disconnect from specific federation or all
  disconnect(federation: string = 'all') {
    if (federation === 'all') {
      // Disconnect all connections
      this.connections.forEach(socket => socket.disconnect());
      this.connections.clear();
    } else {
      // Disconnect specific federation
      const connectionKey = `federation:${federation}`;
      const socket = this.connections.get(connectionKey);
      if (socket) {
        socket.disconnect();
        this.connections.delete(connectionKey);
      }
    }
  }
  
  // Subscribe to an event for a specific federation
  subscribe(eventType: RealtimeEvent, callback: Function, config: WebSocketConfig = {}) {
    const { federationId = 'global' } = config;
    const connectionKey = `federation:${federationId}`;
    
    // Initialize nested maps if needed
    if (!this.listeners.has(connectionKey)) {
      this.listeners.set(connectionKey, new Map());
    }
    
    const federationListeners = this.listeners.get(connectionKey)!;
    
    if (!federationListeners.has(eventType)) {
      federationListeners.set(eventType, new Set());
    }
    
    federationListeners.get(eventType)!.add(callback);
    
    // Connect if not already connected
    this.connect(config);
    
    // Return unsubscribe function
    return () => this.unsubscribe(eventType, callback, { federationId });
  }
  
  // Unsubscribe from events
  unsubscribe(eventType: RealtimeEvent, callback: Function, config: WebSocketConfig = {}) {
    const { federationId = 'global' } = config;
    const connectionKey = `federation:${federationId}`;
    
    if (!this.listeners.has(connectionKey)) return;
    
    const federationListeners = this.listeners.get(connectionKey)!;
    
    if (!federationListeners.has(eventType)) return;
    
    const eventListeners = federationListeners.get(eventType)!;
    eventListeners.delete(callback);
    
    // Clean up empty sets
    if (eventListeners.size === 0) {
      federationListeners.delete(eventType);
    }
    
    // Disconnect if no more listeners for this federation
    if (federationListeners.size === 0) {
      this.listeners.delete(connectionKey);
      this.disconnect(federationId);
    }
  }
  
  // Check connection status for a federation
  isConnected(federationId: string = 'global'): boolean {
    const connectionKey = `federation:${federationId}`;
    const socket = this.connections.get(connectionKey);
    return socket?.connected || false;
  }
  
  // Get last update time for a federation's event
  getLastUpdated(eventType: RealtimeEvent, federationId: string = 'global'): Date | null {
    const key = `federation:${federationId}:${eventType}`;
    return this.lastUpdated.get(key) || null;
  }
}

// Create a singleton instance
export const realtimeService = new RealtimeService();

// React hook for subscribing to real-time events with federation support
export function useRealtimeEvent<T>(
  eventType: RealtimeEvent,
  config: WebSocketConfig = {},
  initialData: T[] = []
): {
  data: T[];
  lastUpdated: Date | null;
  isConnected: boolean;
  federationId: string;
} {
  const { federationId = 'global', authToken } = config;
  const [data, setData] = useState<T[]>(initialData);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  useEffect(() => {
    // Check connection status periodically
    const checkConnectionStatus = () => {
      setIsConnected(realtimeService.isConnected(federationId));
      setLastUpdated(realtimeService.getLastUpdated(eventType, federationId));
    };

    // Subscribe to the event with federation config
    const unsubscribe = realtimeService.subscribe(
      eventType, 
      (newData: T) => {
        setData(currentData => [newData, ...currentData]);
        setLastUpdated(new Date());
      },
      { federationId, authToken }
    );

    // Status check interval
    const statusInterval = setInterval(checkConnectionStatus, 5000);
    
    // Initial connection
    realtimeService.connect({ federationId, authToken });
    checkConnectionStatus();

    // Cleanup on unmount
    return () => {
      unsubscribe();
      clearInterval(statusInterval);
    };
  }, [eventType, federationId, authToken]);

  return { data, lastUpdated, isConnected, federationId };
} 