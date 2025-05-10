'use client';

import { useState, useEffect } from 'react';
import { SigningRequest } from '../lib/signing-api';

interface SignatureConfirmationProps {
  request: SigningRequest;
  onConfirm: () => void;
  onReject: () => void;
}

export default function SignatureConfirmation({ request, onConfirm, onReject }: SignatureConfirmationProps) {
  const [expanded, setExpanded] = useState(false);
  const [countdown, setCountdown] = useState(30); // Auto-reject after 30 seconds
  
  useEffect(() => {
    if (countdown <= 0) {
      onReject();
      return;
    }
    
    const timer = setTimeout(() => setCountdown(countdown - 1), 1000);
    return () => clearTimeout(timer);
  }, [countdown, onReject]);
  
  // Format the message for display
  const formatMessage = () => {
    const { params } = request;
    
    if (params.message) {
      if (typeof params.message === 'string') {
        return params.message;
      } else if (Array.isArray(params.message)) {
        return `<binary data: ${params.message.length} bytes>`;
      }
      return '<binary data>';
    } else if (params.payload) {
      try {
        return JSON.stringify(params.payload, null, 2);
      } catch {
        return String(params.payload);
      }
    }
    
    return '<no data>';
  };
  
  // Format the origin for display
  const formatOrigin = () => {
    try {
      const url = new URL(request.origin);
      return url.hostname;
    } catch {
      return request.origin;
    }
  };
  
  // Format the scope for display
  const formatScope = () => {
    const { scope } = request.params;
    if (!scope) return 'None';
    
    if (scope.federation) return `Federation: ${scope.federation}`;
    if (scope.coop) return `Cooperative: ${scope.coop}`;
    if (scope.community) return `Community: ${scope.community}`;
    
    return 'Unknown';
  };
  
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-lg max-w-md w-full p-6 space-y-4">
        <div className="flex items-start justify-between">
          <h3 className="text-lg font-medium">Signature Request</h3>
          <span className="text-sm text-gray-500 dark:text-gray-400">
            {countdown}s
          </span>
        </div>
        
        <div>
          <p className="text-sm text-gray-600 dark:text-gray-300 mb-2">
            <span className="font-semibold">{formatOrigin()}</span> is requesting your signature
          </p>
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
            Scope: {formatScope()}
          </p>
          
          <div className="border border-gray-200 dark:border-gray-700 rounded p-3 bg-gray-50 dark:bg-gray-900">
            <div className="flex justify-between items-center">
              <span className="text-xs font-medium">Message to sign</span>
              <button 
                onClick={() => setExpanded(!expanded)} 
                className="text-xs text-blue-600 dark:text-blue-400"
              >
                {expanded ? 'Collapse' : 'Expand'}
              </button>
            </div>
            
            <pre className={`mt-2 text-xs overflow-auto ${expanded ? 'max-h-40' : 'max-h-20'}`}>
              {formatMessage()}
            </pre>
          </div>
        </div>
        
        <div className="flex space-x-3 pt-2">
          <button
            onClick={onReject}
            className="flex-1 py-2 border border-gray-300 dark:border-gray-600 rounded text-sm font-medium"
          >
            Reject
          </button>
          <button
            onClick={onConfirm}
            className="flex-1 py-2 bg-icn-primary text-white rounded text-sm font-medium hover:bg-icn-secondary"
          >
            Sign
          </button>
        </div>
      </div>
    </div>
  );
} 