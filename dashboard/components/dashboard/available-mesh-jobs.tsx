import React, { useEffect, useState } from 'react';
import { MeshJob } from '@/types/mesh'; // Assuming types are in @/types/mesh

// Mock shadcn/ui components - replace with actual imports
const Table = ({ children }: { children: React.ReactNode }) => <table className="w-full border-collapse text-sm">{children}</table>;
const TableHeader = ({ children }: { children: React.ReactNode }) => <thead className="bg-gray-50">{children}</thead>;
const TableRow = ({ children, className }: { children: React.ReactNode, className?: string }) => <tr className={`border-b ${className || ''}`}>{children}</tr>;
const TableHead = ({ children }: { children: React.ReactNode }) => <th className="px-4 py-2 text-left font-semibold text-gray-600">{children}</th>;
const TableBody = ({ children }: { children: React.ReactNode }) => <tbody>{children}</tbody>;
const TableCell = ({ children, className }: { children: React.ReactNode, className?: string }) => <td className={`px-4 py-3 ${className || ''}`}>{children}</td>;
const Button = ({ onClick, children, variant, size, className }: { onClick?: () => void, children: React.ReactNode, variant?: string, size?: string, className?: string }) => 
    <button onClick={onClick} className={`px-3 py-1 rounded text-xs ${variant === 'outline' ? 'border border-gray-200 hover:bg-gray-50' : 'bg-blue-500 text-white hover:bg-blue-600'} ${className || ''}`}>{children}</button>;
// End mock shadcn/ui components

// Mock API fetcher function
const fetchAvailableJobs = async (): Promise<MeshJob[]> => {
    console.log('Fetching available jobs...');
    return new Promise(resolve => {
        setTimeout(() => {
            resolve([
                {
                    job_id: 'job_avail_alpha_001',
                    originator_did: 'did:icn:peer_node_X:abcdef12345',
                    params: {
                        wasm_cid: 'bafybeigx_available_wasm_1',
                        function_name: 'complex_simulation',
                        required_resources_json: '{"cpu_cores": 2, "memory_gb": 4, "storage_gb": 50, "network_bandwidth_mbps": 100}',
                        qos_profile: 'HighPerformance',
                        max_acceptable_bid_icn: 500,
                    },
                    submitted_at: new Date(Date.now() - 1 * 60 * 60 * 1000).toISOString(), // 1 hour ago
                },
                {
                    job_id: 'job_avail_beta_002',
                    originator_did: 'did:icn:peer_node_Y:uvwxyz67890',
                    params: {
                        wasm_cid: 'bafybeigy_available_wasm_2',
                        function_name: 'data_aggregation',
                        required_resources_json: '{"cpu_cores": 1, "memory_gb": 1, "storage_gb": 10}',
                        qos_profile: 'Standard',
                    },
                    submitted_at: new Date(Date.now() - 30 * 60 * 1000).toISOString(), // 30 minutes ago
                },
            ]);
        }, 1200);
    });
};

export function AvailableMeshJobs() {
    const [jobs, setJobs] = useState<MeshJob[]>([]);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        setIsLoading(true);
        fetchAvailableJobs()
            .then(fetchedJobs => {
                setJobs(fetchedJobs);
            })
            .catch(err => {
                setError(err.message || 'Failed to load available jobs.');
            })
            .finally(() => {
                setIsLoading(false);
            });
    }, []);

    const handleExpressInterest = (jobId: string) => {
        console.log(`User expressed interest in Job ID: ${jobId}. Backend MeshNode handles actual P2P message.`);
        // Here you might show a toast notification or some local UI feedback
        // For example: showToast(`Interest expressed for ${jobId}.`);
    };

    if (isLoading) return <p>Loading available jobs on the mesh...</p>;
    if (error) return <p className="text-red-500">Error: {error}</p>;
    if (jobs.length === 0 && !isLoading) return <p>No jobs currently available on the mesh.</p>;

    return (
        <section>
            <h2 className="text-xl font-semibold mb-4">Available Jobs on the Mesh</h2>
            <div className="overflow-x-auto">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>Job ID</TableHead>
                            <TableHead>Originator DID</TableHead>
                            <TableHead>WASM CID</TableHead>
                            <TableHead>QoS Profile</TableHead>
                            <TableHead>Required Resources</TableHead>
                            <TableHead>Actions</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {jobs.map((job: MeshJob) => (
                            <TableRow key={job.job_id}>
                                <TableCell className="font-mono text-xs">{job.job_id}</TableCell>
                                <TableCell className="font-mono text-xs">{job.originator_did.substring(0, 25)}...</TableCell>
                                <TableCell className="font-mono text-xs">{job.params.wasm_cid.substring(0, 20)}...</TableCell>
                                <TableCell>{job.params.qos_profile}</TableCell>
                                <TableCell className="text-xs max-w-xs overflow-hidden whitespace-nowrap overflow-ellipsis" title={job.params.required_resources_json}>
                                    {job.params.required_resources_json}
                                </TableCell>
                                <TableCell>
                                    <Button 
                                        onClick={() => handleExpressInterest(job.job_id)}
                                        variant="outline"
                                        size="sm"
                                    >
                                        Express Interest
                                    </Button>
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </section>
    );
} 