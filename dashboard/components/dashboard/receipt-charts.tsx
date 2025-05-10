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
  const [receiptData, setReceiptData] = useState<{
    executorData: any[];
    timeSeriesData: any[];
  }>({ executorData: [], timeSeriesData: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Try to fetch from API first
        const data = await ICNApi.getLatestReceipts(50).catch(() => {
          // If API call fails, use mock data
          return getMockData.latestReceipts();
        });
        
        const processedData = processReceiptsForChart(data);
        setReceiptData(processedData);
      } catch (err) {
        setError("Failed to fetch receipt data for charts");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, []);

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
      <CardHeader>
        <CardTitle>Receipt Analytics</CardTitle>
      </CardHeader>
      <CardContent>
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
                      activeDot={{ r: 8, onClick: (e, payload) => handleDateClick(payload.payload) }}
                    />
                    <Line 
                      type="monotone" 
                      dataKey="totalCPU" 
                      stroke="#82ca9d" 
                      name="CPU Usage" 
                      activeDot={{ r: 8, onClick: (e, payload) => handleDateClick(payload.payload) }}
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