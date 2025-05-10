'use client';

import { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Progress } from '../ui/progress'; 
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';

// Types for mesh jobs
interface MeshJob {
  id: string;
  description: string;
  wasmCid: string;
  status: string;
  submitter: string;
  createdAt: string;
  priority: string;
  resourceType: string;
  resourceAmount: number;
}

interface MeshJobReceipt {
  jobId: string;
  executorNodeId: string;
  executorNodeDid: string;
  resultStatus: number;
  startTime: string;
  endTime: string;
  resourceUsage: Array<[string, number]>;
  receiptCid: string;
}

// Mock data for demonstration
const mockJobs: MeshJob[] = [
  {
    id: 'job-1234-abcd',
    description: 'Data analysis job',
    wasmCid: 'bafybeih7q27itb576mtmy5yzggkfzqnfj5dis4h2og6epvyvjyvcedwmze',
    status: 'Completed',
    submitter: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK',
    createdAt: '2023-11-15T10:30:00Z',
    priority: 'medium',
    resourceType: 'compute',
    resourceAmount: 500,
  },
  {
    id: 'job-5678-efgh',
    description: 'Federation metrics calculation',
    wasmCid: 'bafybeiczsscdsbs28wwgixjutbgkwjeilmfggqgjxct2sln63n4kaffcxu',
    status: 'Running',
    submitter: 'did:key:z6MktyAYM2rE5N2h9kYgqSMv9uCWeP9j9JapH5xJd9XwM7oP',
    createdAt: '2023-11-16T08:15:00Z',
    priority: 'high',
    resourceType: 'compute',
    resourceAmount: 800,
  },
  {
    id: 'job-9012-ijkl',
    description: 'Cooperative proposal validation',
    wasmCid: 'bafybeibnsoufr2renruhgqs5ol37nhz4znfnmbrz7ozzavt3qsy3jizoom',
    status: 'Queued',
    submitter: 'did:key:z6MkhFEtyY9Z86W7fPestiYBhJ5SYFNMnJ8cJJ8MVAiEt5Q2',
    createdAt: '2023-11-16T14:45:00Z',
    priority: 'low',
    resourceType: 'compute',
    resourceAmount: 300,
  },
];

const mockResourceUsage = [
  { name: 'Compute', jobs: 12, resources: 4500 },
  { name: 'Storage', jobs: 7, resources: 25000 },
  { name: 'Bandwidth', jobs: 5, resources: 8500 },
];

export function MeshJobs() {
  const [jobs, setJobs] = useState<MeshJob[]>(mockJobs);
  const [selectedJob, setSelectedJob] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<MeshJobReceipt | null>(null);
  const [loading, setLoading] = useState(false);

  // In a real implementation, this would fetch jobs from an API
  useEffect(() => {
    // Simulated WebSocket connection for live updates
    const intervalId = setInterval(() => {
      // Randomly update a job status to simulate progress
      if (Math.random() > 0.7) {
        setJobs(prevJobs => {
          const newJobs = [...prevJobs];
          const randomIndex = Math.floor(Math.random() * newJobs.length);
          const statusChoices = ['Queued', 'Running', 'Completed', 'Failed'];
          const randomStatus = statusChoices[Math.floor(Math.random() * statusChoices.length)];
          newJobs[randomIndex] = { ...newJobs[randomIndex], status: randomStatus };
          return newJobs;
        });
      }
    }, 5000);

    return () => clearInterval(intervalId);
  }, []);

  // Fetch job receipt when a job is selected
  const fetchJobReceipt = (jobId: string) => {
    setLoading(true);
    setSelectedJob(jobId);
    
    // Simulate API call
    setTimeout(() => {
      setReceipt({
        jobId,
        executorNodeId: 'node-xyz-123',
        executorNodeDid: 'did:key:z6MkhFEtyY9Z86W7fPestiYBhJ5SYFNMnJ8cJJ8MVAiEt5Q2',
        resultStatus: 0,
        startTime: '2023-11-16T15:00:00Z',
        endTime: '2023-11-16T15:05:30Z',
        resourceUsage: [['compute', 450], ['storage', 1200]],
        receiptCid: 'receipt-abcdef-123456',
      });
      setLoading(false);
    }, 1000);
  };

  // Format DID for display
  const formatDid = (did: string) => {
    return `${did.substring(0, 15)}...${did.substring(did.length - 5)}`;
  };

  // Status color mapping
  const getStatusColor = (status: string) => {
    switch (status.toLowerCase()) {
      case 'completed': return 'bg-green-500';
      case 'running': return 'bg-blue-500';
      case 'queued': return 'bg-yellow-500';
      case 'failed': return 'bg-red-500';
      default: return 'bg-gray-500';
    }
  };

  return (
    <Tabs defaultValue="jobs">
      <div className="mb-4">
        <TabsList>
          <TabsTrigger value="jobs">Active Jobs</TabsTrigger>
          <TabsTrigger value="stats">Resource Usage</TabsTrigger>
        </TabsList>
      </div>
      
      <TabsContent value="jobs">
        <div className="grid gap-4 grid-cols-1 md:grid-cols-3">
          <Card className="md:col-span-2">
            <CardHeader>
              <CardTitle>Mesh Compute Jobs</CardTitle>
              <CardDescription>
                View and manage distributed computation jobs across the planetary mesh.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="rounded-md border">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Job ID</TableHead>
                      <TableHead>Description</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Priority</TableHead>
                      <TableHead>Action</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {jobs.map((job) => (
                      <TableRow key={job.id}>
                        <TableCell className="font-mono text-xs">{job.id.substring(0, 12)}</TableCell>
                        <TableCell>{job.description}</TableCell>
                        <TableCell>
                          <Badge className={getStatusColor(job.status)}>{job.status}</Badge>
                        </TableCell>
                        <TableCell className="capitalize">{job.priority}</TableCell>
                        <TableCell>
                          <Button 
                            variant="outline" 
                            size="sm"
                            onClick={() => fetchJobReceipt(job.id)}
                          >
                            View Details
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          </Card>
          
          <Card>
            <CardHeader>
              <CardTitle>Job Details</CardTitle>
              <CardDescription>
                {selectedJob ? `Details for job ${selectedJob.substring(0, 8)}...` : 'Select a job to view details'}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {loading ? (
                <div className="flex items-center justify-center h-40">
                  <Progress value={75} className="w-full" />
                  <p className="text-sm text-muted-foreground mt-2">Loading job details...</p>
                </div>
              ) : receipt ? (
                <div className="space-y-4">
                  <div>
                    <h4 className="text-sm font-medium">Receipt ID</h4>
                    <p className="text-sm font-mono">{receipt.receiptCid}</p>
                  </div>
                  <div>
                    <h4 className="text-sm font-medium">Executor Node</h4>
                    <p className="text-sm">{receipt.executorNodeId}</p>
                  </div>
                  <div>
                    <h4 className="text-sm font-medium">Execution Time</h4>
                    <p className="text-sm">
                      {new Date(receipt.endTime).getTime() - new Date(receipt.startTime).getTime()}ms
                    </p>
                  </div>
                  <div>
                    <h4 className="text-sm font-medium">Resource Usage</h4>
                    <ul className="text-sm">
                      {receipt.resourceUsage.map(([type, amount], i) => (
                        <li key={i} className="flex justify-between">
                          <span className="capitalize">{type}</span>
                          <span>{amount} units</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                  <div>
                    <h4 className="text-sm font-medium">Result Status</h4>
                    <Badge className={receipt.resultStatus === 0 ? 'bg-green-500' : 'bg-red-500'}>
                      {receipt.resultStatus === 0 ? 'Success' : `Error (${receipt.resultStatus})`}
                    </Badge>
                  </div>
                </div>
              ) : (
                <div className="flex items-center justify-center h-40 text-muted-foreground">
                  Select a job to view its receipt details
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </TabsContent>
      
      <TabsContent value="stats">
        <Card>
          <CardHeader>
            <CardTitle>Mesh Resource Usage</CardTitle>
            <CardDescription>
              Distribution of resources used by mesh computation jobs.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="h-80">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart
                  data={mockResourceUsage}
                  margin={{ top: 20, right: 30, left: 20, bottom: 5 }}
                >
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="name" />
                  <YAxis yAxisId="left" orientation="left" stroke="#8884d8" />
                  <YAxis yAxisId="right" orientation="right" stroke="#82ca9d" />
                  <Tooltip />
                  <Legend />
                  <Bar yAxisId="left" dataKey="jobs" fill="#8884d8" name="Number of Jobs" />
                  <Bar yAxisId="right" dataKey="resources" fill="#82ca9d" name="Resource Units" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  );
} 