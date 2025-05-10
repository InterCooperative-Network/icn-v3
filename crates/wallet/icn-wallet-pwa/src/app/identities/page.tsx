'use client';

import { useEffect, useState } from 'react';
import Link from 'next/link';
import * as storage from '../../lib/storage';
import CreateIdentity from '../../components/CreateIdentity';

interface Identity {
  did: string;
  name?: string;
  context?: string;
  createdAt: number;
}

export default function IdentitiesPage() {
  const [identities, setIdentities] = useState<Identity[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  
  useEffect(() => {
    loadIdentities();
  }, []);
  
  async function loadIdentities() {
    try {
      setLoading(true);
      const db = await storage.getDb();
      const identitiesData = await db.getAll('keypairs');
      
      setIdentities(identitiesData.map(identity => ({
        did: identity.did,
        name: identity.name,
        context: identity.context,
        createdAt: identity.createdAt,
      })));
    } catch (err) {
      setError('Failed to load identities');
      console.error('Error loading identities:', err);
    } finally {
      setLoading(false);
    }
  }
  
  const handleCreateSuccess = (did: string) => {
    setShowCreate(false);
    loadIdentities();
  };
  
  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString();
  };
  
  const shortenDid = (did: string) => {
    if (did.length <= 30) return did;
    return `${did.substring(0, 15)}...${did.substring(did.length - 10)}`;
  };
  
  return (
    <main className="container mx-auto p-4 md:p-8">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold mb-6">My Identities</h1>
        
        <button
          onClick={() => setShowCreate(!showCreate)}
          className="rounded bg-icn-primary px-4 py-2 text-white hover:bg-icn-secondary focus:outline-none focus:ring-2 focus:ring-icn-accent"
        >
          {showCreate ? 'Cancel' : 'Create New'}
        </button>
      </div>
      
      {showCreate && (
        <div className="my-6 rounded-lg border border-gray-200 bg-white p-6 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <CreateIdentity onSuccess={handleCreateSuccess} />
        </div>
      )}
      
      {error && (
        <div className="my-4 rounded-md bg-red-50 p-4 dark:bg-red-900/30">
          <p className="text-sm text-red-800 dark:text-red-200">{error}</p>
        </div>
      )}
      
      {loading ? (
        <div className="flex items-center justify-center py-12">
          <div className="h-6 w-6 animate-spin rounded-full border-2 border-gray-300 border-t-icn-primary"></div>
          <span className="ml-2">Loading identities...</span>
        </div>
      ) : identities.length === 0 ? (
        <div className="rounded-lg border border-gray-200 bg-white p-8 text-center dark:border-gray-700 dark:bg-gray-800">
          <p className="mb-4 text-gray-600 dark:text-gray-300">You don't have any identities yet.</p>
          {!showCreate && (
            <button
              onClick={() => setShowCreate(true)}
              className="rounded bg-icn-primary px-4 py-2 text-white hover:bg-icn-secondary focus:outline-none focus:ring-2 focus:ring-icn-accent"
            >
              Create Your First Identity
            </button>
          )}
        </div>
      ) : (
        <div className="mt-4 grid gap-6 md:grid-cols-2 lg:grid-cols-3">
          {identities.map((identity) => (
            <div
              key={identity.did}
              className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm dark:border-gray-700 dark:bg-gray-800"
            >
              <div className="mb-2 flex items-center justify-between">
                <h3 className="font-semibold">
                  {identity.name || 'Unnamed Identity'}
                </h3>
                <span className="text-xs rounded-full bg-gray-100 px-2 py-1 text-gray-600 dark:bg-gray-700 dark:text-gray-300">
                  {identity.context || 'No Context'}
                </span>
              </div>
              
              <p className="mb-4 mt-2 text-sm font-mono text-gray-500 dark:text-gray-400">
                {shortenDid(identity.did)}
              </p>
              
              <div className="mt-4 flex items-center justify-between text-xs text-gray-500 dark:text-gray-400">
                <span>Created {formatDate(identity.createdAt)}</span>
                
                <Link
                  href={`/identities/${encodeURIComponent(identity.did)}`}
                  className="text-icn-accent hover:underline"
                >
                  View Details
                </Link>
              </div>
            </div>
          ))}
        </div>
      )}
    </main>
  );
} 