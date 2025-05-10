'use client';

import { useEffect, useState } from 'react';
import Link from 'next/link';
import * as storage from '../lib/storage';

export default function Home() {
  const [isInstalled, setIsInstalled] = useState(false);
  const [identityCount, setIdentityCount] = useState<number | null>(null);
  
  useEffect(() => {
    // Check if the app is installed as PWA
    if (window.matchMedia('(display-mode: standalone)').matches) {
      setIsInstalled(true);
    }
    
    // Get identity count
    async function getIdentityCount() {
      try {
        const db = await storage.getDb();
        const identities = await db.getAll('keypairs');
        setIdentityCount(identities.length);
      } catch (err) {
        console.error('Error getting identity count:', err);
      }
    }
    
    getIdentityCount();
  }, []);

  return (
    <main className="flex min-h-screen flex-col items-center justify-between p-6 md:p-24">
      <div className="z-10 w-full max-w-md">
        <h1 className="mb-6 text-center text-3xl font-bold">ICN Wallet</h1>
        
        <div className="rounded-lg bg-white p-6 shadow dark:bg-slate-800">
          <p className="my-4 text-center">
            Secure identity and credential management for the Internet of Cooperation Network
          </p>
          
          {!isInstalled && (
            <div className="mt-4 rounded-md bg-blue-50 p-3 dark:bg-blue-900/30">
              <p className="text-sm">
                Install this application on your device for offline access and better experience.
              </p>
            </div>
          )}
          
          <div className="mt-8 space-y-4">
            <Link 
              href="/identities"
              className="block w-full rounded bg-icn-primary px-4 py-3 text-center font-medium text-white hover:bg-icn-secondary focus:outline-none focus:ring-2 focus:ring-icn-accent"
            >
              {identityCount === 0 
                ? 'Create Your First Identity' 
                : identityCount === 1 
                  ? 'Manage Your Identity' 
                  : identityCount !== null 
                    ? `Manage Your Identities (${identityCount})` 
                    : 'Manage Identities'}
            </Link>
            
            <Link 
              href="/credentials"
              className="block w-full rounded border border-icn-primary px-4 py-3 text-center font-medium text-icn-primary hover:bg-icn-primary/10 focus:outline-none focus:ring-2 focus:ring-icn-accent"
            >
              View Credentials
            </Link>
            
            <Link 
              href="/settings"
              className="mt-6 block text-center text-sm text-gray-500 hover:text-icn-primary hover:underline dark:text-gray-400"
            >
              Settings
            </Link>
          </div>
        </div>
      </div>
    </main>
  );
} 