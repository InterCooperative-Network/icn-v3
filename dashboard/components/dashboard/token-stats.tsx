"use client";

import { useState, useEffect } from "react";
import { 
  Card, 
  CardContent, 
  CardHeader, 
  CardTitle 
} from "../ui/card";
import { ICNApi, getMockData, Token } from "../../lib/api";

export function TokenStats() {
  const [tokens, setTokens] = useState<Token[]>([]);
  const [stats, setStats] = useState<{
    total_minted: number;
    total_burnt: number;
    active_accounts: number;
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Try to fetch token balances from API first
        const tokenData = await ICNApi.getTokenBalances().catch(() => {
          // If API call fails, use mock data
          return getMockData.tokenBalances();
        });
        
        // Try to fetch token stats from API first
        const statsData = await ICNApi.getTokenStats().catch(() => {
          // If API call fails, use mock data
          return getMockData.tokenStats();
        });
        
        setTokens(tokenData);
        setStats(statsData);
      } catch (err) {
        setError("Failed to fetch token data");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, []);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Token Statistics</CardTitle>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="flex justify-center items-center h-40">
            <div className="text-slate-500">Loading...</div>
          </div>
        ) : error ? (
          <div className="text-red-500">{error}</div>
        ) : (
          <div className="space-y-6">
            {/* Token Economy Overview */}
            {stats && (
              <div className="grid grid-cols-3 gap-4">
                <div className="bg-blue-50 p-4 rounded-lg text-center">
                  <div className="text-lg font-bold text-blue-600">{stats.total_minted}</div>
                  <div className="text-sm text-slate-600">Total Minted</div>
                </div>
                <div className="bg-red-50 p-4 rounded-lg text-center">
                  <div className="text-lg font-bold text-red-600">{stats.total_burnt}</div>
                  <div className="text-sm text-slate-600">Total Burnt</div>
                </div>
                <div className="bg-green-50 p-4 rounded-lg text-center">
                  <div className="text-lg font-bold text-green-600">{stats.active_accounts}</div>
                  <div className="text-sm text-slate-600">Active Accounts</div>
                </div>
              </div>
            )}
            
            {/* Top Token Holders */}
            <div>
              <h3 className="text-md font-semibold mb-2">Top Token Holders</h3>
              <div className="space-y-2">
                {tokens.slice(0, 5).map((token, index) => (
                  <div key={index} className="flex justify-between items-center p-2 border-b border-slate-200">
                    <div className="font-mono text-sm truncate max-w-[200px]">{token.did}</div>
                    <div className="font-semibold">{token.balance}</div>
                  </div>
                ))}
              </div>
            </div>
            
            <div className="text-sm text-blue-600 hover:underline cursor-pointer text-center mt-2">
              View all accounts
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
} 