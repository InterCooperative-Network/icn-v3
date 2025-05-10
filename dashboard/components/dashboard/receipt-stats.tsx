"use client";

import { useState, useEffect } from "react";
import { 
  Card, 
  CardContent, 
  CardHeader, 
  CardTitle 
} from "../ui/card";
import { formatDate, formatCID } from "../../lib/utils";
import { ExecutionReceipt, ICNApi, getMockData } from "../../lib/api";

export function ReceiptStats() {
  const [receipts, setReceipts] = useState<ExecutionReceipt[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Try to fetch from API first
        const data = await ICNApi.getLatestReceipts(5).catch(() => {
          // If API call fails, use mock data
          return getMockData.latestReceipts().slice(0, 5);
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

  return (
    <Card>
      <CardHeader>
        <CardTitle>Latest Execution Receipts</CardTitle>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="flex justify-center items-center h-40">
            <div className="text-slate-500">Loading...</div>
          </div>
        ) : error ? (
          <div className="text-red-500">{error}</div>
        ) : (
          <div className="space-y-4">
            {receipts.map((receipt, index) => (
              <div key={index} className="flex flex-col space-y-2 p-4 border border-slate-200 rounded-md">
                <div className="flex justify-between">
                  <div className="font-medium">Task CID</div>
                  <div className="text-blue-600 font-mono text-sm">
                    {formatCID(receipt.task_cid)}
                  </div>
                </div>
                <div className="flex justify-between">
                  <div className="font-medium">Executor</div>
                  <div className="text-blue-600 font-mono text-sm">
                    {formatCID(receipt.executor)}
                  </div>
                </div>
                <div className="flex justify-between">
                  <div className="font-medium">Timestamp</div>
                  <div className="text-slate-600 text-sm">
                    {formatDate(receipt.timestamp)}
                  </div>
                </div>
                <div className="mt-2 pt-2 border-t border-slate-200">
                  <div className="font-medium mb-2">Resource Usage</div>
                  <div className="grid grid-cols-3 gap-2">
                    {Object.entries(receipt.resource_usage).map(([resource, amount]) => (
                      <div key={resource} className="bg-blue-50 p-2 rounded text-center">
                        <div className="text-xs text-slate-600">{resource}</div>
                        <div className="font-semibold">{amount}</div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            ))}
            
            <div className="text-sm text-blue-600 hover:underline cursor-pointer text-center mt-2">
              View all receipts
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
} 