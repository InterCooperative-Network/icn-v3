"use client";

import { useState, useEffect } from "react";
import Layout from '../../components/layout';
import { Card, CardContent, CardHeader, CardTitle } from '../../components/ui/card';
import { ICNApi, getMockData, Token } from "../../lib/api";
import { formatCID } from "../../lib/utils";
import { TokenCharts } from '../../components/dashboard/token-charts';

export default function TokensPage() {
  const [tokens, setTokens] = useState<Token[]>([]);
  const [stats, setStats] = useState<{
    total_minted: number;
    total_burnt: number;
    active_accounts: number;
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"table" | "chart">("table");

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
    <Layout>
      <div className="space-y-6">
        <h1 className="text-3xl font-bold">Token Ledger</h1>
        <p className="text-slate-600">
          View token balances and economic metrics for the ICN network.
        </p>
        
        {/* View toggle */}
        <div className="flex">
          <div className="flex rounded-md shadow-sm">
            <button
              onClick={() => setView("table")}
              className={`px-4 py-2 text-sm font-medium rounded-l-md ${
                view === "table"
                  ? "bg-blue-600 text-white"
                  : "bg-white text-gray-700 hover:bg-gray-50"
              } border border-gray-300`}
            >
              Table View
            </button>
            <button
              onClick={() => setView("chart")}
              className={`px-4 py-2 text-sm font-medium rounded-r-md ${
                view === "chart"
                  ? "bg-blue-600 text-white"
                  : "bg-white text-gray-700 hover:bg-gray-50"
              } border border-gray-300 border-l-0`}
            >
              Chart View
            </button>
          </div>
        </div>
        
        {/* Token metrics */}
        {stats && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <Card>
              <CardContent className="pt-6">
                <div className="text-center">
                  <h3 className="text-lg font-medium text-slate-700">Total Supply</h3>
                  <p className="text-3xl font-bold text-blue-600 mt-2">{stats.total_minted - stats.total_burnt}</p>
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="text-center">
                  <h3 className="text-lg font-medium text-slate-700">Total Minted</h3>
                  <p className="text-3xl font-bold text-green-600 mt-2">{stats.total_minted}</p>
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="text-center">
                  <h3 className="text-lg font-medium text-slate-700">Total Burnt</h3>
                  <p className="text-3xl font-bold text-red-600 mt-2">{stats.total_burnt}</p>
                </div>
              </CardContent>
            </Card>
          </div>
        )}
        
        {/* Token chart or table view */}
        {view === "chart" ? (
          <TokenCharts />
        ) : (
          <Card>
            <CardHeader>
              <CardTitle>Token Balances</CardTitle>
            </CardHeader>
            <CardContent>
              {loading ? (
                <div className="flex justify-center items-center h-40">
                  <div className="text-slate-500">Loading...</div>
                </div>
              ) : error ? (
                <div className="text-red-500">{error}</div>
              ) : tokens.length === 0 ? (
                <div className="text-center py-8 text-slate-500">
                  No token balances found.
                </div>
              ) : (
                <div className="space-y-4">
                  <div className="overflow-x-auto">
                    <table className="w-full border-collapse">
                      <thead>
                        <tr className="bg-slate-100">
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Account (DID)</th>
                          <th className="px-4 py-2 text-right text-sm font-medium text-slate-600">Balance</th>
                          <th className="px-4 py-2 text-right text-sm font-medium text-slate-600">% of Supply</th>
                        </tr>
                      </thead>
                      <tbody>
                        {tokens.map((token, index) => {
                          const totalSupply = stats ? stats.total_minted - stats.total_burnt : 0;
                          const percentage = totalSupply > 0 
                            ? ((token.balance / totalSupply) * 100).toFixed(2) 
                            : "0.00";
                          
                          return (
                            <tr key={index} className={index % 2 === 0 ? "bg-white" : "bg-slate-50"}>
                              <td className="px-4 py-2 text-sm font-mono">{token.did}</td>
                              <td className="px-4 py-2 text-sm text-right font-medium">{token.balance.toLocaleString()}</td>
                              <td className="px-4 py-2 text-sm text-right">{percentage}%</td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </Layout>
  );
} 