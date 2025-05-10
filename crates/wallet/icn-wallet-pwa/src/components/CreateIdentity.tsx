'use client';

import { useState } from 'react';
import * as crypto from '../lib/crypto';
import * as storage from '../lib/storage';

interface CreateIdentityProps {
  onSuccess?: (did: string) => void;
}

export default function CreateIdentity({ onSuccess }: CreateIdentityProps) {
  const [name, setName] = useState('');
  const [context, setContext] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const handleCreate = async () => {
    try {
      setError(null);
      setIsCreating(true);
      
      // Generate a new keypair
      const keypair = await crypto.generateKeyPair();
      
      // Store it in the database
      await storage.storeKeypair(keypair, name || undefined, context || undefined);
      
      // Call success callback if provided
      if (onSuccess) {
        onSuccess(keypair.did);
      }
      
      // Reset form
      setName('');
      setContext('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create identity');
      console.error('Error creating identity:', err);
    } finally {
      setIsCreating(false);
    }
  };
  
  return (
    <div className="space-y-4">
      <h2 className="text-xl font-semibold">Create New Identity</h2>
      
      {error && (
        <div className="rounded-md bg-red-50 p-4 dark:bg-red-900/30">
          <p className="text-sm text-red-800 dark:text-red-200">
            {error}
          </p>
        </div>
      )}
      
      <div className="space-y-4">
        <div>
          <label htmlFor="identity-name" className="mb-1 block text-sm font-medium">
            Identity Name (optional)
          </label>
          <input
            id="identity-name"
            type="text"
            className="w-full rounded-md border border-gray-300 p-2 dark:border-gray-700 dark:bg-gray-800"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Main Identity"
          />
        </div>
        
        <div>
          <label htmlFor="identity-context" className="mb-1 block text-sm font-medium">
            Organization Context (optional)
          </label>
          <input
            id="identity-context"
            type="text"
            className="w-full rounded-md border border-gray-300 p-2 dark:border-gray-700 dark:bg-gray-800"
            value={context}
            onChange={(e) => setContext(e.target.value)}
            placeholder="federation:example or coop:example"
          />
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Scopes this identity to a specific federation, cooperative, or community
          </p>
        </div>
        
        <button
          onClick={handleCreate}
          disabled={isCreating}
          className="w-full rounded bg-icn-primary py-2 px-4 font-medium text-white hover:bg-icn-secondary focus:outline-none focus:ring-2 focus:ring-icn-accent disabled:opacity-50"
        >
          {isCreating ? 'Creating...' : 'Create Identity'}
        </button>
      </div>
    </div>
  );
} 