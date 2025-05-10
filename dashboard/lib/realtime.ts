import { io, Socket } from 'socket.io-client';
import { useState, useEffect } from 'react';
import { ExecutionReceipt, Token, TokenTransaction } from './api';

// WebSocket connection URL - this should point to your WebSocket server
const SOCKET_URL = process.env.NEXT_PUBLIC_SOCKET_URL || 'http://localhost:8081';

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
  private socket: Socket | null = null;
  private listeners: Map<string, Set<Function>> = new Map();
  private connected: boolean = false;
  private reconnectAttempts: number = 0;
  private maxReconnectAttempts: number = 5;
  
  // Last update timestamps
  private lastUpdated: Map<string, Date> = new Map();

  // Initialize and connect to WebSocket server
  connect() {
    if (this.socket) return;

    this.socket = io(SOCKET_URL, {
      reconnectionAttempts: this.maxReconnectAttempts,
      reconnectionDelay: 1000,
      transports: ['websocket']
    });

    this.socket.on('connect', () => {
      console.log('Connected to real-time updates');
      this.connected = true;
      this.reconnectAttempts = 0;
    });

    this.socket.on('disconnect', () => {
      console.log('Disconnected from real-time updates');
      this.connected = false;
    });

    this.socket.on('connect_error', (err) => {
      console.warn('WebSocket connection error:', err);
      this.reconnectAttempts++;
      
      if (this.reconnectAttempts > this.maxReconnectAttempts) {
        console.warn('Maximum reconnect attempts reached, falling back to polling');
        this.socket?.disconnect();
      }
    });

    // Set up listeners for different event types
    Object.values(RealtimeEvent).forEach(eventType => {
      this.socket?.on(eventType, (data) => {
        this.lastUpdated.set(eventType, new Date());
        const listeners = this.listeners.get(eventType);
        if (listeners) {
          listeners.forEach(callback => callback(data));
        }
      });
    });
  }

  // Disconnect from WebSocket server
  disconnect() {
    if (this.socket) {
      this.socket.disconnect();
      this.socket = null;
      this.connected = false;
    }
  }

  // Subscribe to an event type
  subscribe(eventType: RealtimeEvent, callback: Function) {
    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, new Set());
    }
    this.listeners.get(eventType)?.add(callback);

    // Connect if not already connected
    if (!this.socket) {
      this.connect();
    }

    return () => this.unsubscribe(eventType, callback);
  }

  // Unsubscribe from an event type
  unsubscribe(eventType: RealtimeEvent, callback: Function) {
    const listeners = this.listeners.get(eventType);
    if (listeners) {
      listeners.delete(callback);
      if (listeners.size === 0) {
        this.listeners.delete(eventType);
      }
    }

    // If no more listeners, disconnect
    if (this.listeners.size === 0) {
      this.disconnect();
    }
  }

  // Get the last update time for an event type
  getLastUpdated(eventType: RealtimeEvent): Date | null {
    return this.lastUpdated.get(eventType) || null;
  }

  // Check if currently connected
  isConnected(): boolean {
    return this.connected;
  }
}

// Create a singleton instance
export const realtimeService = new RealtimeService();

// React hook for subscribing to real-time events
export function useRealtimeEvent<T>(
  eventType: RealtimeEvent,
  initialData: T[] = []
): {
  data: T[];
  lastUpdated: Date | null;
  isConnected: boolean;
} {
  const [data, setData] = useState<T[]>(initialData);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  useEffect(() => {
    // Update connection status when it changes
    const checkConnectionStatus = () => {
      setIsConnected(realtimeService.isConnected());
      setLastUpdated(realtimeService.getLastUpdated(eventType));
    };

    // Subscribe to the event
    const unsubscribe = realtimeService.subscribe(eventType, (newData: T) => {
      setData(currentData => [newData, ...currentData]);
      setLastUpdated(new Date());
    });

    // Check connection status regularly
    const statusInterval = setInterval(checkConnectionStatus, 5000);
    
    // Initial connection
    realtimeService.connect();
    checkConnectionStatus();

    // Cleanup
    return () => {
      unsubscribe();
      clearInterval(statusInterval);
    };
  }, [eventType]);

  return { data, lastUpdated, isConnected };
} 