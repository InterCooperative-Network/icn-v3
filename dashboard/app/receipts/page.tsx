"use client";

import { useState, useEffect } from "react";
import Layout from '../../components/layout';
import { Card, CardContent, CardHeader, CardTitle } from '../../components/ui/card';
import { ExecutionReceipt, ICNApi, getMockData } from "../../lib/api";
import { formatDate, formatCID } from "../../lib/utils";
import { ReceiptCharts } from '../../components/dashboard/receipt-charts';

export default function ReceiptsPage() {
  const [receipts, setReceipts] = useState<ExecutionReceipt[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState("");
  const [view, setView] = useState<"table" | "chart">("table");

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Try to fetch from API first
        const data = await ICNApi.getLatestReceipts(20).catch(() => {
          // If API call fails, use mock data
          return getMockData.latestReceipts();
        });
        
        setReceipts(data);
      } catch (err) {
        setError("Failed to fetch receipt data");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, []);

  // Filter receipts based on search term
  const filteredReceipts = receipts.filter(receipt => 
    receipt.task_cid.toLowerCase().includes(searchTerm.toLowerCase()) ||
    receipt.executor.toLowerCase().includes(searchTerm.toLowerCase())
  );

  return (
    <Layout>
      <div className="space-y-6">
        <h1 className="text-3xl font-bold">Execution Receipts</h1>
        <p className="text-slate-600">
          View and search all execution receipts in the ICN network.
        </p>
        
        {/* View toggle and search */}
        <div className="flex flex-col sm:flex-row justify-between gap-4">
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
          
          <div className="relative flex-grow max-w-xl">
            <input
              type="text"
              placeholder="Search by task CID or executor DID..."
              className="w-full px-4 py-2 border border-slate-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
            />
            <div className="absolute inset-y-0 right-0 flex items-center pr-3">
              <svg 
                xmlns="http://www.w3.org/2000/svg" 
                className="h-5 w-5 text-slate-400" 
                fill="none" 
                viewBox="0 0 24 24" 
                stroke="currentColor"
              >
                <path 
                  strokeLinecap="round" 
                  strokeLinejoin="round" 
                  strokeWidth={2} 
                  d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" 
                />
              </svg>
            </div>
          </div>
        </div>
        
        {view === "chart" ? (
          <ReceiptCharts />
        ) : (
          <Card>
            <CardHeader>
              <CardTitle>All Receipts</CardTitle>
            </CardHeader>
            <CardContent>
              {loading ? (
                <div className="flex justify-center items-center h-40">
                  <div className="text-slate-500">Loading...</div>
                </div>
              ) : error ? (
                <div className="text-red-500">{error}</div>
              ) : filteredReceipts.length === 0 ? (
                <div className="text-center py-8 text-slate-500">
                  No receipts found matching your search criteria.
                </div>
              ) : (
                <div className="space-y-4">
                  <div className="overflow-x-auto">
                    <table className="w-full border-collapse">
                      <thead>
                        <tr className="bg-slate-100">
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Task CID</th>
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Executor</th>
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Timestamp</th>
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">CPU</th>
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Memory</th>
                          <th className="px-4 py-2 text-left text-sm font-medium text-slate-600">Storage</th>
                        </tr>
                      </thead>
                      <tbody>
                        {filteredReceipts.map((receipt, index) => (
                          <tr key={index} className={index % 2 === 0 ? "bg-white" : "bg-slate-50"}>
                            <td className="px-4 py-2 text-sm font-mono">{formatCID(receipt.task_cid)}</td>
                            <td className="px-4 py-2 text-sm font-mono">{formatCID(receipt.executor)}</td>
                            <td className="px-4 py-2 text-sm">{formatDate(receipt.timestamp)}</td>
                            <td className="px-4 py-2 text-sm">{receipt.resource_usage.CPU || "-"}</td>
                            <td className="px-4 py-2 text-sm">{receipt.resource_usage.Memory || "-"}</td>
                            <td className="px-4 py-2 text-sm">{receipt.resource_usage.Storage || "-"}</td>
                          </tr>
                        ))}
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