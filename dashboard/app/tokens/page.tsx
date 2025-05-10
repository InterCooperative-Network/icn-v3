"use client";

import { useState, useEffect } from "react";
import { useSearchParams } from "next/navigation";
import Layout from '../../components/layout';
import { Card, CardContent, CardHeader, CardTitle } from '../../components/ui/card';
import { ICNApi, getMockData, Token, TokenTransaction, TokenFilter } from "../../lib/api";
import { formatCID, formatDate } from "../../lib/utils";
import { TokenCharts } from '../../components/dashboard/token-charts';

export default function TokensPage() {
  const searchParams = useSearchParams();
  const dateParam = searchParams.get('date');
  const didParam = searchParams.get('account') || searchParams.get('did');
  
  const [tokens, setTokens] = useState<Token[]>([]);
  const [transactions, setTransactions] = useState<TokenTransaction[]>([]);
  const [stats, setStats] = useState<{
    total_minted: number;
    total_burnt: number;
    active_accounts: number;
    daily_volume?: number;
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"table" | "chart">("table");
  const [filterInfo, setFilterInfo] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Create filter object from URL parameters
        const filter: TokenFilter = {};
        if (dateParam) filter.date = dateParam;
        if (didParam) filter.did = didParam;
        
        // Try to fetch token balances from API first
        let tokenData;
        let transactionData: TokenTransaction[] = [];
        let statsData;
        
        try {
          // Get balances
          tokenData = await ICNApi.getTokenBalances(filter);
          
          // Get transactions if we have a date or account filter
          if (dateParam || didParam) {
            transactionData = await ICNApi.getTokenTransactions(filter);
          }
          
          // Get stats
          statsData = await ICNApi.getTokenStats(filter);
        } catch (err) {
          // If API calls fail, use mock data
          tokenData = getMockData.tokenBalances();
          
          if (dateParam || didParam) {
            transactionData = getMockData.tokenTransactions(filter);
          }
          
          statsData = getMockData.tokenStats(filter);
        }
        
        setTokens(tokenData);
        setTransactions(transactionData);
        setStats(statsData);
      } catch (err) {
        setError("Failed to fetch token data");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, [dateParam, didParam]);

  // Apply URL parameter filters
  useEffect(() => {
    if (dateParam) {
      setFilterInfo(`Showing token activity on ${dateParam}`);
    } else if (didParam) {
      setFilterInfo(`Showing details for account ${formatCID(didParam)}`);
    } else {
      setFilterInfo(null);
    }
  }, [dateParam, didParam]);

  // Clear any active filters
  const clearFilters = () => {
    // This will trigger a page navigation without the query parameters
    window.history.pushState({}, '', '/tokens');
    setFilterInfo(null);
  };

  return (
    <Layout>
      <div className="space-y-6">
        <h1 className="text-3xl font-bold">Token Ledger</h1>
        <p className="text-slate-600">
          View token balances and economic metrics for the ICN network.
        </p>
        
        {/* Filter information */}
        {filterInfo && (
          <div className="bg-blue-50 border-l-4 border-blue-500 p-4 flex justify-between items-center">
            <div>
              <p className="text-blue-700">{filterInfo}</p>
            </div>
            <button 
              onClick={clearFilters}
              className="text-sm px-3 py-1 bg-white border border-blue-300 rounded-md hover:bg-blue-50 text-blue-600"
            >
              Clear Filter
            </button>
          </div>
        )}
        
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
        
        {/* Daily Stats - show when filtered by date */}
        {stats && dateParam && stats.daily_volume !== undefined && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <Card>
              <CardContent className="pt-6">
                <div className="text-center">
                  <h3 className="text-lg font-medium text-slate-700">Daily Volume</h3>
                  <p className="text-3xl font-bold text-blue-600 mt-2">{stats.daily_volume}</p>
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="text-center">
                  <h3 className="text-lg font-medium text-slate-700">Active Accounts</h3>
                  <p className="text-3xl font-bold text-green-600 mt-2">{stats.active_accounts}</p>
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="text-center">
                  <h3 className="text-lg font-medium text-slate-700">Transactions</h3>
                  <p className="text-3xl font-bold text-purple-600 mt-2">{transactions.length}</p>
                </div>
              </CardContent>
            </Card>
          </div>
        )}
        
        {/* Token metrics - hide if filtered to a specific date or account */}
        {stats && !dateParam && !didParam && (
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
        {view === "chart" && !didParam ? (
          <TokenCharts />
        ) : (
          <Card>
            <CardHeader>
              <CardTitle>
                {didParam ? `Account Details: ${formatCID(didParam)}` : 
                 dateParam ? `Token Activity on ${dateParam}` : 
                 "Token Balances"}
              </CardTitle>
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
                  No token balances found matching your criteria.
                </div>
              ) : (
                <div className="space-y-6">
                  {/* Balances table */}
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
                  
                  {/* Transaction history section */}
                  {(dateParam || didParam) && transactions.length > 0 && (
                    <div className="mt-8">
                      <h3 className="text-lg font-medium mb-4">
                        {dateParam ? "Transactions on this date" : "Account Transactions"}
                      </h3>
                      <div className="overflow-x-auto">
                        <table className="w-full border-collapse">
                          <thead>
                            <tr className="bg-slate-100">
                              <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Transaction ID</th>
                              <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">From</th>
                              <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">To</th>
                              <th className="px-4 py-2 text-right text-sm font-medium text-slate-600">Amount</th>
                              <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Type</th>
                              <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Timestamp</th>
                            </tr>
                          </thead>
                          <tbody>
                            {transactions.map((tx, index) => (
                              <tr key={index} className={index % 2 === 0 ? "bg-white" : "bg-slate-50"}>
                                <td className="px-4 py-2 text-sm font-mono">{tx.id || `-`}</td>
                                <td className="px-4 py-2 text-sm font-mono">
                                  {formatCID(tx.from)}
                                </td>
                                <td className="px-4 py-2 text-sm font-mono">
                                  {formatCID(tx.to)}
                                </td>
                                <td className="px-4 py-2 text-sm text-right">{tx.amount.toLocaleString()}</td>
                                <td className="px-4 py-2 text-sm">
                                  <span className={`px-2 py-1 rounded-full text-xs ${
                                    tx.operation === "mint" 
                                      ? "bg-green-100 text-green-800" 
                                      : tx.operation === "burn"
                                      ? "bg-red-100 text-red-800"
                                      : "bg-blue-100 text-blue-800"
                                  }`}>
                                    {tx.operation || "transfer"}
                                  </span>
                                </td>
                                <td className="px-4 py-2 text-sm">{formatDate(tx.timestamp)}</td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </Layout>
  );
} 