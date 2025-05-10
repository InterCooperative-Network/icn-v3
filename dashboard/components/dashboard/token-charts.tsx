"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { 
  PieChart, 
  Pie, 
  Cell, 
  Tooltip, 
  Legend, 
  ResponsiveContainer,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid
} from "recharts";
import { 
  Card, 
  CardContent, 
  CardHeader, 
  CardTitle 
} from "../ui/card";
import { ICNApi, getMockData, Token } from "../../lib/api";

// Mock historical data since our API doesn't provide it yet
const generateMockHistoricalData = () => {
  const data = [];
  const now = new Date();
  const totalDays = 30;
  
  let totalSupply = 45000;
  let mintedToday = 0;
  let burntToday = 0;
  
  for (let i = 0; i < totalDays; i++) {
    const date = new Date();
    date.setDate(now.getDate() - (totalDays - i - 1));
    
    // Random daily changes
    mintedToday = Math.floor(Math.random() * 500) + 100;
    burntToday = Math.floor(Math.random() * 300);
    
    totalSupply += (mintedToday - burntToday);
    
    data.push({
      date: date.toISOString().split('T')[0],
      totalSupply,
      minted: mintedToday,
      burnt: burntToday
    });
  }
  
  return data;
};

const COLORS = ['#0088FE', '#00C49F', '#FFBB28', '#FF8042', '#8884D8'];

export function TokenCharts() {
  const router = useRouter();
  const [tokens, setTokens] = useState<Token[]>([]);
  const [historicalData, setHistoricalData] = useState<any[]>([]);
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
        
        // Generate mock historical data
        // This would be replaced with API data when available
        const histData = generateMockHistoricalData();
        
        setTokens(tokenData);
        setHistoricalData(histData);
      } catch (err) {
        setError("Failed to fetch token data");
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
      // Navigate to tokens page with date filter
      router.push(`/tokens?date=${data.date}`);
    }
  };

  const handleHolderClick = (data: any) => {
    if (data && data.did) {
      // Navigate to tokens page with holder filter
      router.push(`/tokens?account=${data.did}`);
    }
  };

  // Prepare data for pie chart - take top 4 holders and group the rest as "Others"
  const prepareDistributionData = (tokens: Token[]) => {
    // Sort tokens by balance (descending)
    const sortedTokens = [...tokens].sort((a, b) => b.balance - a.balance);
    
    const pieData = [];
    let othersTotal = 0;
    
    // Take top 4 holders
    for (let i = 0; i < sortedTokens.length; i++) {
      if (i < 4) {
        pieData.push({
          name: sortedTokens[i].did.slice(-10), // Truncate DID for display
          value: sortedTokens[i].balance,
          did: sortedTokens[i].did  // Store full DID for click handling
        });
      } else {
        // Group the rest as "Others"
        othersTotal += sortedTokens[i].balance;
      }
    }
    
    if (othersTotal > 0) {
      pieData.push({
        name: "Others",
        value: othersTotal,
        did: null
      });
    }
    
    return pieData;
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Token Analytics</CardTitle>
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
              <h3 className="text-lg font-medium mb-2">Token Supply History</h3>
              <p className="text-sm text-slate-500 mb-2">Click on a data point to see token activity for that date</p>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart
                    data={historicalData}
                    margin={{ top: 10, right: 30, left: 0, bottom: 0 }}
                    onClick={(e) => e && e.activePayload && handleDateClick(e.activePayload[0].payload)}
                  >
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="date" />
                    <YAxis />
                    <Tooltip cursor={{ strokeDasharray: '3 3' }} />
                    <Legend />
                    <Area 
                      type="monotone" 
                      dataKey="totalSupply" 
                      stackId="1"
                      stroke="#8884d8" 
                      fill="#8884d8" 
                      name="Total Supply"
                    />
                    <Area 
                      type="monotone" 
                      dataKey="minted" 
                      stackId="2"
                      stroke="#82ca9d" 
                      fill="#82ca9d" 
                      name="Daily Minted" 
                    />
                    <Area 
                      type="monotone" 
                      dataKey="burnt" 
                      stackId="2"
                      stroke="#ffc658" 
                      fill="#ffc658" 
                      name="Daily Burnt" 
                    />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </div>
            
            <div>
              <h3 className="text-lg font-medium mb-2">Token Distribution</h3>
              <p className="text-sm text-slate-500 mb-2">Click on a segment to see account details</p>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <PieChart>
                    <Pie
                      data={prepareDistributionData(tokens)}
                      cx="50%"
                      cy="50%"
                      labelLine={false}
                      outerRadius={80}
                      fill="#8884d8"
                      dataKey="value"
                      label={({ name, percent }) => `${name} (${(percent * 100).toFixed(0)}%)`}
                      onClick={(data) => data.did && handleHolderClick(data)}
                    >
                      {prepareDistributionData(tokens).map((entry, index) => (
                        <Cell 
                          key={`cell-${index}`} 
                          fill={COLORS[index % COLORS.length]} 
                          cursor={entry.did ? "pointer" : "default"}
                        />
                      ))}
                    </Pie>
                    <Tooltip formatter={(value) => `${value} tokens`} />
                    <Legend onClick={(entry) => {
                      const matchedData = prepareDistributionData(tokens).find(item => item.name === entry.value);
                      if (matchedData && matchedData.did) {
                        handleHolderClick(matchedData);
                      }
                    }} />
                  </PieChart>
                </ResponsiveContainer>
              </div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
} 