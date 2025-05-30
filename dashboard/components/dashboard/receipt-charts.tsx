"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { 
  LineChart, 
  Line, 
  XAxis, 
  YAxis, 
  CartesianGrid, 
  Tooltip, 
  Legend, 
  ResponsiveContainer,
  BarChart,
  Bar
} from "recharts";
import { 
  Card, 
  CardContent, 
  CardHeader, 
  CardTitle 
} from "../ui/card";
import { ExecutionReceipt, ICNApi, getMockData } from "../../lib/api";
import { useRealtimeEvent, RealtimeEvent } from "../../lib/realtime";
import { FederationSelector } from "../ui/federation-selector";

// Function to process receipts for visualization
const processReceiptsForChart = (receipts: ExecutionReceipt[]) => {
  // Calculate resource usage by executor
  const executorStats: Record<string, { 
    executor: string, 
    executorDid: string,
    CPU: number, 
    Memory: number, 
    Storage: number,
    count: number 
  }> = {};
  
  // Process time-based data (last 7 days)
  const sevenDaysAgo = new Date();
  sevenDaysAgo.setDate(sevenDaysAgo.getDate() - 7);
  
  const dailyData: Record<string, { 
    date: string, 
    count: number,
    totalCPU: number,
    totalMemory: number 
  }> = {};
  
  // Process each receipt
  receipts.forEach(receipt => {
    // For executor stats
    const executor = receipt.executor;
    if (!executorStats[executor]) {
      executorStats[executor] = {
        executor: executor.slice(executor.length - 10),
        executorDid: executor,
        CPU: 0,
        Memory: 0,
        Storage: 0,
        count: 0
      };
    }
    
    executorStats[executor].CPU += receipt.resource_usage.CPU || 0;
    executorStats[executor].Memory += receipt.resource_usage.Memory || 0;
    executorStats[executor].Storage += receipt.resource_usage.Storage || 0;
    executorStats[executor].count += 1;
    
    // For daily data
    const date = new Date(receipt.timestamp);
    if (date >= sevenDaysAgo) {
      const dateStr = date.toISOString().split('T')[0];
      if (!dailyData[dateStr]) {
        dailyData[dateStr] = {
          date: dateStr,
          count: 0,
          totalCPU: 0,
          totalMemory: 0
        };
      }
      
      dailyData[dateStr].count += 1;
      dailyData[dateStr].totalCPU += receipt.resource_usage.CPU || 0;
      dailyData[dateStr].totalMemory += receipt.resource_usage.Memory || 0;
    }
  });
  
  // Convert to arrays for charts
  const executorData = Object.values(executorStats);
  const timeSeriesData = Object.values(dailyData).sort((a, b) => 
    new Date(a.date).getTime() - new Date(b.date).getTime()
  );
  
  return { executorData, timeSeriesData };
};

export function ReceiptCharts() {
  const router = useRouter();
  const [selectedFederation, setSelectedFederation] = useState<string>('global');
  const [receiptData, setReceiptData] = useState<{
    executorData: any[];
    timeSeriesData: any[];
  }>({ executorData: [], timeSeriesData: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  // Subscribe to real-time receipt updates with federation scoping
  const { 
    data: newReceipts, 
    lastUpdated, 
    isConnected, 
    federationId 
  } = useRealtimeEvent<ExecutionReceipt>(
    RealtimeEvent.RECEIPT_CREATED,
    { 
      federationId: selectedFederation,
      // In a real app, you would get this from an auth service
      authToken: selectedFederation !== 'global' ? 'your-jwt-token' : undefined
    },
    []
  );

  // Handle federation selection
  const handleFederationChange = (federation: string) => {
    setSelectedFederation(federation);
    setLoading(true);
    
    // Fetch data for the selected federation
    fetchData(federation);
  };

  const fetchData = async (federation: string = 'global') => {
    try {
      // In a real implementation, you would pass the federation ID to the API
      // Here we're just using the mock data for demonstration
      const data = await ICNApi.getLatestReceipts(50).catch(() => {
        return getMockData.latestReceipts();
      });
      
      // Filter receipts by federation if not global
      const filteredData = federation === 'global' 
        ? data 
        : data.filter((receipt: ExecutionReceipt) => 
            // This assumes the receipt has a federation_id field
            // You might need to adjust based on your actual data structure
            (receipt as any).federation_id === federation || 
            receipt.executor.includes(federation)
          );
      
      const processedData = processReceiptsForChart(filteredData);
      setReceiptData(processedData);
    } catch (err) {
      setError("Failed to fetch receipt data for charts");
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData(selectedFederation);
  }, []);
  
  // Update charts when new receipts come in via WebSocket
  useEffect(() => {
    if (newReceipts.length > 0) {
      // Get current data
      const allReceipts = [...newReceipts];
      
      // Process the combined data
      const processedData = processReceiptsForChart(allReceipts);
      
      // Update the state with merged data
      setReceiptData(prevData => {
        // Merge the new executor data with existing data
        const mergedExecutorData = [...prevData.executorData];
        processedData.executorData.forEach(newItem => {
          const existingIndex = mergedExecutorData.findIndex(item => item.executorDid === newItem.executorDid);
          if (existingIndex >= 0) {
            // Update existing executor data
            mergedExecutorData[existingIndex] = {
              ...mergedExecutorData[existingIndex],
              CPU: mergedExecutorData[existingIndex].CPU + newItem.CPU,
              Memory: mergedExecutorData[existingIndex].Memory + newItem.Memory,
              Storage: mergedExecutorData[existingIndex].Storage + newItem.Storage,
              count: mergedExecutorData[existingIndex].count + newItem.count
            };
          } else {
            // Add new executor data
            mergedExecutorData.push(newItem);
          }
        });
        
        // Merge the time series data
        const mergedTimeSeriesData = [...prevData.timeSeriesData];
        processedData.timeSeriesData.forEach(newItem => {
          const existingIndex = mergedTimeSeriesData.findIndex(item => item.date === newItem.date);
          if (existingIndex >= 0) {
            // Update existing time series data
            mergedTimeSeriesData[existingIndex] = {
              ...mergedTimeSeriesData[existingIndex],
              count: mergedTimeSeriesData[existingIndex].count + newItem.count,
              totalCPU: mergedTimeSeriesData[existingIndex].totalCPU + newItem.totalCPU,
              totalMemory: mergedTimeSeriesData[existingIndex].totalMemory + newItem.totalMemory
            };
          } else {
            // Add new time series data
            mergedTimeSeriesData.push(newItem);
          }
        });
        
        // Return the merged data
        return {
          executorData: mergedExecutorData,
          timeSeriesData: mergedTimeSeriesData.sort((a, b) => 
            new Date(a.date).getTime() - new Date(b.date).getTime()
          )
        };
      });
    }
  }, [newReceipts]);

  // Handle clicks on chart data points
  const handleDateClick = (data: any) => {
    if (data && data.date) {
      // Navigate to receipts page with date filter
      router.push(`/receipts?date=${data.date}`);
    }
  };

  const handleExecutorClick = (data: any) => {
    if (data && data.executorDid) {
      // Navigate to receipts page with executor filter
      router.push(`/receipts?executor=${data.executorDid}`);
    }
  };

  return (
    <Card>
      <CardHeader className="pb-0">
        <CardTitle className="flex justify-between items-center">
          <span>Receipt Analytics</span>
          {lastUpdated && (
            <span className="text-xs text-slate-500 flex items-center">
              <span className={`inline-block w-2 h-2 rounded-full mr-2 ${isConnected ? 'bg-green-500' : 'bg-red-500'}`}></span>
              Last updated: {new Date(lastUpdated).toLocaleTimeString()}
            </span>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <FederationSelector 
          onSelect={handleFederationChange} 
          federations={['global', 'federation1', 'federation2']}
          initialFederation={selectedFederation}
        />
        
        {loading ? (
          <div className="flex justify-center items-center h-40">
            <div className="text-slate-500">Loading...</div>
          </div>
        ) : error ? (
          <div className="text-red-500">{error}</div>
        ) : (
          <div className="space-y-8">
            <div>
              <h3 className="text-lg font-medium mb-2">Daily Receipt Volume</h3>
              <p className="text-sm text-slate-500 mb-2">Click on a data point to see receipts for that date</p>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart
                    data={receiptData.timeSeriesData}
                    margin={{ top: 5, right: 30, left: 20, bottom: 5 }}
                    onClick={(e) => e && e.activePayload && handleDateClick(e.activePayload[0].payload)}
                  >
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" />
                    <YAxis />
                    <Tooltip cursor={{ strokeDasharray: '3 3' }} />
                    <Legend />
                    <Line 
                      type="monotone" 
                      dataKey="count" 
                      stroke="#8884d8" 
                      name="Receipts"
                      activeDot={{ 
                        r: 8, 
                        onClick: (e: any, payload: any) => handleDateClick(payload) 
                      }}
                    />
                    <Line 
                      type="monotone" 
                      dataKey="totalCPU" 
                      stroke="#82ca9d" 
                      name="CPU Usage" 
                      activeDot={{ 
                        r: 8, 
                        onClick: (e: any, payload: any) => handleDateClick(payload) 
                      }}
                    />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            </div>
            
            <div>
              <h3 className="text-lg font-medium mb-2">Resource Usage by Executor</h3>
              <p className="text-sm text-slate-500 mb-2">Click on a bar to see receipts for that executor</p>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart
                    data={receiptData.executorData}
                    margin={{ top: 5, right: 30, left: 20, bottom: 5 }}
                    onClick={(e) => e && e.activePayload && handleExecutorClick(e.activePayload[0].payload)}
                  >
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="executor" />
                    <YAxis />
                    <Tooltip cursor={{ fill: 'rgba(0, 0, 0, 0.1)' }} />
                    <Legend />
                    <Bar dataKey="CPU" fill="#8884d8" name="CPU" />
                    <Bar dataKey="Memory" fill="#82ca9d" name="Memory" />
                    <Bar dataKey="Storage" fill="#ffc658" name="Storage" />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
} 