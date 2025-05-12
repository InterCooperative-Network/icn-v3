import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, Select, MenuItem, FormControl, InputLabel, Box, Typography, CircularProgress } from '@mui/material';
import { fetchReputationActivity } from '../lib/api';
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
  Bar,
  Cell
} from 'recharts';

type ReputationEvent = {
  eventId: string;
  timestamp: number;
  subject: string; // DID of the node receiving the reputation update
  issuer: string;  // DID of the node issuing the reputation update (runtime)
  eventType: 'JobCompletedSuccessfully' | 'JobFailed' | 'DishonestyPenalty';
  receiptId?: string;
  scoreImpact: number;
};

type TimeRange = '1h' | '24h' | '7d' | '30d';

const timeRangeOptions: {value: TimeRange; label: string}[] = [
  { value: '1h', label: 'Last Hour' },
  { value: '24h', label: 'Last 24 Hours' },
  { value: '7d', label: 'Last 7 Days' },
  { value: '30d', label: 'Last 30 Days' }
];

const ReputationActivityChart: React.FC = () => {
  const [timeRange, setTimeRange] = useState<TimeRange>('24h');
  const [events, setEvents] = useState<ReputationEvent[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  
  useEffect(() => {
    const loadData = async () => {
      setLoading(true);
      setError(null);
      
      try {
        const data = await fetchReputationActivity(timeRange);
        setEvents(data);
      } catch (err) {
        console.error('Failed to load reputation activity data:', err);
        setError('Failed to load reputation data');
      } finally {
        setLoading(false);
      }
    };
    
    loadData();
  }, [timeRange]);
  
  // Process data for time series chart
  const timeSeriesData = React.useMemo(() => {
    if (!events.length) return [];
    
    // Group events by hour or day depending on time range
    const groupByTime = (events: ReputationEvent[]) => {
      const result: {[key: string]: {time: string; successful: number; failed: number; dishonesty: number; total: number}} = {};
      
      const format = timeRange === '1h' ? 
        (timestamp: number) => new Date(timestamp).toISOString().slice(11, 16) : // HH:MM
        (timestamp: number) => new Date(timestamp).toISOString().slice(5, 10);   // MM-DD
        
      events.forEach(event => {
        const timeKey = format(event.timestamp);
        if (!result[timeKey]) {
          result[timeKey] = {
            time: timeKey,
            successful: 0,
            failed: 0,
            dishonesty: 0,
            total: 0
          };
        }
        
        result[timeKey].total += 1;
        
        if (event.eventType === 'JobCompletedSuccessfully') {
          result[timeKey].successful += 1;
        } else if (event.eventType === 'JobFailed') {
          result[timeKey].failed += 1;
        } else if (event.eventType === 'DishonestyPenalty') {
          result[timeKey].dishonesty += 1;
        }
      });
      
      return Object.values(result).sort((a, b) => a.time.localeCompare(b.time));
    };
    
    return groupByTime(events);
  }, [events, timeRange]);
  
  // Process data for score impact chart
  const scoreImpactData = React.useMemo(() => {
    if (!events.length) return [];
    
    // Group events by node (subject DID)
    const result: {[key: string]: {subject: string; scoreImpact: number; eventCount: number}} = {};
    
    events.forEach(event => {
      // Use a shortened version of the DID for display
      const shortDID = event.subject.substring(0, 12) + '...' + event.subject.substring(event.subject.length - 4);
      
      if (!result[event.subject]) {
        result[event.subject] = {
          subject: shortDID,
          scoreImpact: 0,
          eventCount: 0
        };
      }
      
      result[event.subject].scoreImpact += event.scoreImpact;
      result[event.subject].eventCount += 1;
    });
    
    // Return top 10 nodes by absolute score impact
    return Object.values(result)
      .sort((a, b) => Math.abs(b.scoreImpact) - Math.abs(a.scoreImpact))
      .slice(0, 10);
  }, [events]);
  
  const handleTimeRangeChange = (event: React.ChangeEvent<{ value: unknown }>) => {
    setTimeRange(event.target.value as TimeRange);
  };
  
  if (loading) {
    return (
      <Card>
        <CardHeader title="Reputation Activity" />
        <CardContent>
          <Box display="flex" justifyContent="center" alignItems="center" minHeight="300px">
            <CircularProgress />
          </Box>
        </CardContent>
      </Card>
    );
  }
  
  if (error) {
    return (
      <Card>
        <CardHeader title="Reputation Activity" />
        <CardContent>
          <Box display="flex" justifyContent="center" alignItems="center" minHeight="300px">
            <Typography color="error">{error}</Typography>
          </Box>
        </CardContent>
      </Card>
    );
  }
  
  if (!events.length) {
    return (
      <Card>
        <CardHeader 
          title="Reputation Activity" 
          action={
            <FormControl variant="outlined" size="small">
              <InputLabel>Time Range</InputLabel>
              <Select
                value={timeRange}
                onChange={handleTimeRangeChange}
                label="Time Range"
              >
                {timeRangeOptions.map(option => (
                  <MenuItem key={option.value} value={option.value}>
                    {option.label}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          }
        />
        <CardContent>
          <Box display="flex" justifyContent="center" alignItems="center" minHeight="300px">
            <Typography>No reputation events in the selected time range</Typography>
          </Box>
        </CardContent>
      </Card>
    );
  }
  
  return (
    <Card>
      <CardHeader 
        title="Reputation Activity" 
        action={
          <FormControl variant="outlined" size="small">
            <InputLabel>Time Range</InputLabel>
            <Select
              value={timeRange}
              onChange={handleTimeRangeChange}
              label="Time Range"
            >
              {timeRangeOptions.map(option => (
                <MenuItem key={option.value} value={option.value}>
                  {option.label}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        }
      />
      <CardContent>
        <Typography variant="h6" gutterBottom>
          Reputation Events Over Time
        </Typography>
        <Box height={300} mb={4}>
          <ResponsiveContainer width="100%" height="100%">
            <LineChart
              data={timeSeriesData}
              margin={{ top: 5, right: 30, left: 20, bottom: 5 }}
            >
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="time" />
              <YAxis />
              <Tooltip />
              <Legend />
              <Line type="monotone" dataKey="successful" stroke="#4caf50" name="Successful Jobs" />
              <Line type="monotone" dataKey="failed" stroke="#f44336" name="Failed Jobs" />
              <Line type="monotone" dataKey="dishonesty" stroke="#ff9800" name="Dishonesty Events" />
              <Line type="monotone" dataKey="total" stroke="#2196f3" name="Total Events" />
            </LineChart>
          </ResponsiveContainer>
        </Box>
        
        <Typography variant="h6" gutterBottom>
          Node Reputation Impact (Top 10)
        </Typography>
        <Box height={300}>
          <ResponsiveContainer width="100%" height="100%">
            <BarChart
              data={scoreImpactData}
              margin={{ top: 5, right: 30, left: 20, bottom: 5 }}
              layout="vertical"
            >
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis type="number" />
              <YAxis dataKey="subject" type="category" width={150} />
              <Tooltip
                formatter={(value: number) => [value.toFixed(2), 'Score Impact']}
                labelFormatter={(value) => `Node: ${value}`}
              />
              <Legend />
              <Bar 
                dataKey="scoreImpact" 
                name="Score Impact"
                fill="#8884d8"
              >
                {scoreImpactData.map((entry, index) => (
                  <Cell 
                    key={`cell-${index}`} 
                    fill={entry.scoreImpact >= 0 ? '#4caf50' : '#f44336'} 
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </Box>
      </CardContent>
    </Card>
  );
};

export default ReputationActivityChart; 