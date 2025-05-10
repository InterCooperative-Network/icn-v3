'use client';

import { useState, useEffect } from 'react';
import { useSigningContext } from '../../components/SigningProvider';
import Link from 'next/link';

export default function SigningDemo() {
  const { activeDid, trustOrigin } = useSigningContext();
  const [message, setMessage] = useState('Hello, ICN Federation!');
  const [scope, setScope] = useState('federation:example');
  const [origin, setOrigin] = useState('https://dashboard.icn.dev');
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);
  
  // Automatically trust the demo origin
  useEffect(() => {
    trustOrigin(window.location.origin);
  }, [trustOrigin]);
  
  // Simulate requesting a signature from an external application
  const simulateSignatureRequest = async () => {
    setError(null);
    setResult(null);
    
    try {
      // Create a unique ID for this request
      const requestId = `sig-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
      
      // Parse the scope
      let scopeObj = {};
      if (scope) {
        const [type, id] = scope.split(':');
        if (type && id) {
          scopeObj = { [type]: id };
        }
      }
      
      // Create the request
      const request = {
        id: requestId,
        action: 'sign',
        params: {
          message,
          scope: scopeObj,
          did: activeDid
        },
        requireConfirmation: true
      };
      
      // Set up response handler
      const responseHandler = (event: MessageEvent) => {
        const response = event.data;
        
        // Check if this is a response to our request
        if (response && response.id === requestId) {
          // Clean up event listener
          window.removeEventListener('message', responseHandler);
          
          if (response.success) {
            setResult(response.result);
          } else {
            setError(response.error || 'Unknown error');
          }
        }
      };
      
      // Listen for the response
      window.addEventListener('message', responseHandler);
      
      // Send the request to the wallet (in this case, to ourselves)
      window.postMessage(request, window.location.origin);
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Unknown error');
    }
  };
  
  // Simulate getting available scopes
  const simulateGetScopes = async () => {
    setError(null);
    setResult(null);
    
    try {
      // Create a unique ID for this request
      const requestId = `scopes-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
      
      // Create the request
      const request = {
        id: requestId,
        action: 'getScopes',
        params: {},
        requireConfirmation: false
      };
      
      // Set up response handler
      const responseHandler = (event: MessageEvent) => {
        const response = event.data;
        
        // Check if this is a response to our request
        if (response && response.id === requestId) {
          // Clean up event listener
          window.removeEventListener('message', responseHandler);
          
          if (response.success) {
            setResult(response.result);
          } else {
            setError(response.error || 'Unknown error');
          }
        }
      };
      
      // Listen for the response
      window.addEventListener('message', responseHandler);
      
      // Send the request to the wallet (in this case, to ourselves)
      window.postMessage(request, window.location.origin);
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Unknown error');
    }
  };
  
  // Add a trusted origin
  const handleAddTrustedOrigin = () => {
    if (!origin) return;
    
    try {
      // Validate URL
      new URL(origin);
      trustOrigin(origin);
      alert(`Origin ${origin} trusted`);
    } catch (error) {
      setError('Invalid URL format');
    }
  };
  
  return (
    <main className="container mx-auto p-4 md:p-8">
      <div className="mb-6">
        <Link href="/" className="text-icn-accent hover:underline">
          &larr; Back to Home
        </Link>
        <h1 className="text-2xl font-bold mt-2">ICN Wallet Signing API Demo</h1>
        <p className="text-gray-600 dark:text-gray-300 mt-2">
          Test the signing API functionality in this demo page
        </p>
      </div>
      
      <div className="grid gap-6 md:grid-cols-2">
        <div className="rounded-lg border border-gray-200 bg-white p-6 shadow dark:border-gray-700 dark:bg-gray-800">
          <h2 className="mb-4 text-xl font-semibold">Simulate Signing Request</h2>
          
          <div className="space-y-4">
            <div>
              <label className="mb-1 block text-sm font-medium">Active DID</label>
              <div className="rounded bg-gray-100 p-2 text-sm font-mono dark:bg-gray-700">
                {activeDid || 'No active DID'}
              </div>
            </div>
            
            <div>
              <label htmlFor="message" className="mb-1 block text-sm font-medium">
                Message to Sign
              </label>
              <textarea
                id="message"
                rows={3}
                className="w-full rounded border border-gray-300 p-2 dark:border-gray-600 dark:bg-gray-700"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
              />
            </div>
            
            <div>
              <label htmlFor="scope" className="mb-1 block text-sm font-medium">
                Organizational Scope
              </label>
              <input
                id="scope"
                type="text"
                className="w-full rounded border border-gray-300 p-2 dark:border-gray-600 dark:bg-gray-700"
                value={scope}
                onChange={(e) => setScope(e.target.value)}
                placeholder="federation:example"
              />
              <p className="mt-1 text-xs text-gray-500">
                Format: federation:id, coop:id, or community:id
              </p>
            </div>
            
            <div className="flex space-x-3 pt-2">
              <button
                onClick={simulateSignatureRequest}
                className="flex-1 rounded bg-icn-primary py-2 text-white hover:bg-icn-secondary"
              >
                Sign Message
              </button>
              
              <button
                onClick={simulateGetScopes}
                className="flex-1 rounded border border-icn-primary py-2 text-icn-primary hover:bg-icn-primary/10"
              >
                Get Scopes
              </button>
            </div>
          </div>
        </div>
        
        <div className="rounded-lg border border-gray-200 bg-white p-6 shadow dark:border-gray-700 dark:bg-gray-800">
          <h2 className="mb-4 text-xl font-semibold">Trust Origin</h2>
          
          <div className="space-y-4">
            <div>
              <label htmlFor="origin" className="mb-1 block text-sm font-medium">
                Origin URL
              </label>
              <input
                id="origin"
                type="text"
                className="w-full rounded border border-gray-300 p-2 dark:border-gray-600 dark:bg-gray-700"
                value={origin}
                onChange={(e) => setOrigin(e.target.value)}
                placeholder="https://dashboard.icn.dev"
              />
              <p className="mt-1 text-xs text-gray-500">
                Enter the full origin URL including protocol
              </p>
            </div>
            
            <button
              onClick={handleAddTrustedOrigin}
              className="w-full rounded bg-icn-primary py-2 text-white hover:bg-icn-secondary"
            >
              Trust Origin
            </button>
            
            <div className="mt-6">
              <h3 className="mb-2 text-lg font-medium">Results</h3>
              
              {error && (
                <div className="mb-4 rounded-md bg-red-50 p-3 text-sm text-red-700 dark:bg-red-900/30 dark:text-red-200">
                  {error}
                </div>
              )}
              
              {result && (
                <div className="rounded-md bg-gray-50 p-3 dark:bg-gray-700">
                  <pre className="text-xs overflow-auto max-h-60">
                    {JSON.stringify(result, null, 2)}
                  </pre>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </main>
  );
} 