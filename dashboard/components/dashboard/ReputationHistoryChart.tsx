'use client';

import React, { useEffect, useState } from 'react';
import { fetchReputationHistory, ReputationDataPoint } from '@/lib/api';
import {
    LineChart,
    Line,
    XAxis,
    YAxis,
    CartesianGrid,
    Tooltip,
    Legend,
    ResponsiveContainer,
} from 'recharts';

interface ReputationHistoryChartProps {
    did: string;
}

interface ChartDataPoint {
    timestamp: number; // Unix timestamp in seconds
    formattedTime: string;
    score: number;
}

// Helper to format timestamp to a readable date/time string
const formatTimestamp = (timestamp: number): string => {
    const date = new Date(timestamp * 1000); // Convert seconds to milliseconds
    // Simple date string, customize as needed (e.g., to include time)
    return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
};

export function ReputationHistoryChart({ did }: ReputationHistoryChartProps) {
    const [historyData, setHistoryData] = useState<ChartDataPoint[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!did) return;

        async function loadHistory() {
            try {
                setLoading(true);
                setError(null);
                const rawData: ReputationDataPoint[] = await fetchReputationHistory(did);
                
                const formattedData = rawData.map(point => ({
                    timestamp: point[0],
                    formattedTime: formatTimestamp(point[0]),
                    score: point[1],
                }));
                // Sort by timestamp just in case, though backend should send sorted data
                formattedData.sort((a, b) => a.timestamp - b.timestamp);
                setHistoryData(formattedData);
            } catch (e) {
                setError(e instanceof Error ? e.message : 'Unknown error fetching reputation history');
                console.error(`Error fetching reputation history for DID ${did}:`, e);
            } finally {
                setLoading(false);
            }
        }

        loadHistory();
    }, [did]);

    if (loading) {
        return <div className="p-4 text-center text-sm">Loading reputation history...</div>;
    }

    if (error) {
        return <div className="p-4 text-center text-red-500 text-sm">Error: {error}</div>;
    }

    if (historyData.length === 0) {
        return <div className="p-4 text-center text-sm">No reputation history found for this DID.</div>;
    }

    return (
        <div style={{ width: '100%', height: 200 }} className="my-4"> {/* Adjust height as needed */}
            <ResponsiveContainer width="100%" height="100%">
                <LineChart
                    data={historyData}
                    margin={{
                        top: 5,
                        right: 20, // Reduced right margin
                        left: -25, // Reduced left margin to pull Y-axis labels closer
                        bottom: 5,
                    }}
                >
                    <CartesianGrid strokeDasharray="3 3" strokeOpacity={0.3} />
                    <XAxis 
                        dataKey="formattedTime" 
                        tick={{ fontSize: 10 }} 
                        angle={-30} // Angle ticks for better readability if many points
                        textAnchor="end"
                        height={40} // Adjust height for angled labels
                        interval="preserveStartEnd" // Show first and last, then sample
                    />
                    <YAxis 
                        domain={[0, 100]} 
                        tick={{ fontSize: 10 }} 
                        allowDataOverflow={false}
                    />
                    <Tooltip
                        contentStyle={{ fontSize: '12px', padding: '2px 8px' }}
                        labelStyle={{ fontWeight: 'bold', marginBottom: '4px' }}
                        formatter={(value: number, name: string) => {
                            if (name === 'Score') return [value.toFixed(1), 'Score'];
                            return [value, name];
                        }}
                        labelFormatter={(label: string, payload?: any[]) => {
                            if (payload && payload.length > 0 && payload[0].payload.timestamp) {
                                const fullDate = new Date(payload[0].payload.timestamp * 1000);
                                return fullDate.toLocaleString(); // More detailed timestamp in tooltip
                            }
                            return label;
                        }}
                    />
                    <Legend wrapperStyle={{ fontSize: '12px'}} />
                    <Line 
                        type="monotone" 
                        dataKey="score" 
                        stroke="#8884d8" 
                        strokeWidth={2}
                        activeDot={{ r: 6 }} 
                        dot={{ r: 2 }}
                        name="Score"
                    />
                </LineChart>
            </ResponsiveContainer>
        </div>
    );
}

export default ReputationHistoryChart; 