'use client';

import { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { signingApi, SigningRequest, LogLevel } from '../lib/signing-api';
import SignatureConfirmation from './SignatureConfirmation';

interface SigningContextType {
  pendingRequests: SigningRequest[];
  activeDid: string | null;
  setActiveDid: (did: string) => Promise<boolean>;
  trustOrigin: (origin: string) => void;
  removeTrustedOrigin: (origin: string) => void;
}

const SigningContext = createContext<SigningContextType>({
  pendingRequests: [],
  activeDid: null,
  setActiveDid: async () => false,
  trustOrigin: () => {},
  removeTrustedOrigin: () => {},
});

export const useSigningContext = () => useContext(SigningContext);

interface SigningProviderProps {
  children: ReactNode;
  logLevel?: LogLevel;
}

export default function SigningProvider({ children, logLevel = LogLevel.INFO }: SigningProviderProps) {
  const [pendingRequests, setPendingRequests] = useState<SigningRequest[]>([]);
  const [activeDid, setActiveDid] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);
  
  // Initialize when the component mounts
  useEffect(() => {
    if (initialized) return;
    
    // Set log level
    signingApi.setLogLevel(logLevel);
    
    // Set up user confirmation callback
    signingApi.setUserConfirmationCallback(async (request) => {
      return new Promise<boolean>((resolve) => {
        // Add this request to the pending requests
        setPendingRequests((current) => [...current, { ...request, resolve }]);
      });
    });
    
    // Load active DID
    const loadActiveDid = async () => {
      try {
        // We're using a dummy request to get the active DID
        const dummyRequest: SigningRequest = {
          id: 'internal-get-did',
          action: 'getDid',
          origin: window.location.origin,
          params: {}
        };
        
        const response = await signingApi.processRequest(dummyRequest);
        if (response.success && response.result?.did) {
          setActiveDid(response.result.did);
        }
      } catch (error) {
        console.error('Failed to load active DID:', error);
      }
    };
    
    loadActiveDid();
    setInitialized(true);
  }, [initialized, logLevel]);
  
  // Handle a request confirmation
  const handleConfirm = (request: SigningRequest) => {
    const requestWithResolve = pendingRequests.find(r => r.id === request.id);
    if (requestWithResolve && 'resolve' in requestWithResolve) {
      (requestWithResolve as any).resolve(true);
      
      // Remove from pending requests
      setPendingRequests(current => current.filter(r => r.id !== request.id));
    }
  };
  
  // Handle a request rejection
  const handleReject = (request: SigningRequest) => {
    const requestWithResolve = pendingRequests.find(r => r.id === request.id);
    if (requestWithResolve && 'resolve' in requestWithResolve) {
      (requestWithResolve as any).resolve(false);
      
      // Remove from pending requests
      setPendingRequests(current => current.filter(r => r.id !== request.id));
    }
  };
  
  // Set the active DID
  const handleSetActiveDid = async (did: string) => {
    const success = await signingApi.setActiveDid(did);
    if (success) {
      setActiveDid(did);
    }
    return success;
  };
  
  // Add a trusted origin
  const trustOrigin = (origin: string) => {
    signingApi.addTrustedOrigin(origin);
  };
  
  // Remove a trusted origin
  const removeTrustedOrigin = (origin: string) => {
    signingApi.removeTrustedOrigin(origin);
  };
  
  return (
    <SigningContext.Provider 
      value={{
        pendingRequests,
        activeDid,
        setActiveDid: handleSetActiveDid,
        trustOrigin,
        removeTrustedOrigin
      }}
    >
      {children}
      
      {/* Render pending confirmation dialogs */}
      {pendingRequests.map((request) => (
        <SignatureConfirmation
          key={request.id}
          request={request}
          onConfirm={() => handleConfirm(request)}
          onReject={() => handleReject(request)}
        />
      ))}
    </SigningContext.Provider>
  );
} 