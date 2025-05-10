"use client";

import { useState, useEffect } from "react";
import { 
  Card, 
  CardContent, 
  CardHeader, 
  CardTitle 
} from "../ui/card";
import { FederationNode, ICNApi, getMockData } from "../../lib/api";
import { formatCID } from "../../lib/utils";

export function FederationStatus() {
  const [nodes, setNodes] = useState<FederationNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Try to fetch from API first
        const data = await ICNApi.getFederationNodes().catch(() => {
          // If API call fails, use mock data
          return getMockData.federationNodes();
        });
        
        setNodes(data);
      } catch (err) {
        setError("Failed to fetch federation node data");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, []);

  // Count online and offline nodes
  const onlineCount = nodes.filter(node => node.status === "online").length;
  const totalCount = nodes.length;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Federation Status</CardTitle>
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
            {/* Health status */}
            <div className="bg-slate-50 p-4 rounded-lg">
              <div className="flex justify-between items-center">
                <div>
                  <div className="text-sm text-slate-600">Node Status</div>
                  <div className="text-lg font-semibold">
                    {onlineCount} / {totalCount} online
                  </div>
                </div>
                <div className={`h-4 w-4 rounded-full ${
                  onlineCount === totalCount 
                    ? "bg-green-500" 
                    : onlineCount > 0 
                    ? "bg-yellow-500" 
                    : "bg-red-500"
                }`}></div>
              </div>
            </div>
            
            {/* Nodes list */}
            <div className="space-y-3">
              {nodes.map((node, index) => (
                <div key={index} className="p-3 border border-slate-200 rounded-md">
                  <div className="flex justify-between items-center mb-2">
                    <div className="font-medium">{node.node_id}</div>
                    <div className={`px-2 py-1 rounded-full text-xs ${
                      node.status === "online" 
                        ? "bg-green-100 text-green-800" 
                        : "bg-red-100 text-red-800"
                    }`}>
                      {node.status}
                    </div>
                  </div>
                  <div className="text-xs text-slate-500 mb-2 font-mono">
                    {formatCID(node.did, false)}
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs text-slate-600">
                    <div>Memory: {node.capabilities.available_memory_mb} MB</div>
                    <div>CPU: {node.capabilities.available_cpu_cores} cores</div>
                    <div>Storage: {node.capabilities.available_storage_mb} MB</div>
                    <div>Location: {node.capabilities.location || "Unknown"}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
} 