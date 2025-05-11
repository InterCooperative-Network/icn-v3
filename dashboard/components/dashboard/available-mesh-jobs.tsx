import React, { useEffect, useState } from 'react';
import { MeshJob } from '@/types/mesh';

// Mock shadcn/ui components - replace with actual imports
const Table = ({ children }: { children: React.ReactNode }) => <table className="w-full border-collapse text-sm">{children}</table>;
const TableHeader = ({ children }: { children: React.ReactNode }) => <thead className="bg-gray-50">{children}</thead>;
const TableRow = ({ children, className }: { children: React.ReactNode, className?: string }) => <tr className={`border-b ${className || ''}`}>{children}</tr>;
const TableHead = ({ children }: { children: React.ReactNode }) => <th className="px-4 py-2 text-left font-semibold text-gray-600">{children}</th>;
const TableBody = ({ children }: { children: React.ReactNode }) => <tbody>{children}</tbody>;
const TableCell = ({ children, className, ...rest }: { children: React.ReactNode, className?: string, [key: string]: any }) => <td className={`px-4 py-3 ${className || ''}`} {...rest}>{children}</td>;
const Button = ({ onClick, children, variant, size, className }: { onClick?: () => void, children: React.ReactNode, variant?: string, size?: string, className?: string }) => 
    <button onClick={onClick} className={`px-3 py-1 rounded text-xs ${variant === 'outline' ? 'border border-gray-200 hover:bg-gray-50' : 'bg-blue-500 text-white hover:bg-blue-600'} ${className || ''}`}>{children}</button>;
// End mock shadcn/ui components

export function AvailableMeshJobs() {
    const [jobs, setJobs] = useState<MeshJob[]>([]);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const fetchJobs = async () => {
            setIsLoading(true);
            setError(null);
            try {
                const response = await fetch('/api/v1/mesh/jobs/available');
                if (!response.ok) {
                    const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                    throw new Error(errorData.message || `HTTP error! status: ${response.status}`);
                }
                const data: MeshJob[] = await response.json();
                setJobs(data);
            } catch (e: any) {
                setError(e.message || 'Failed to load available jobs.');
            } finally {
                setIsLoading(false);
            }
        };

        fetchJobs();
    }, []); // Empty dependency array means this runs once on mount

    const handleExpressInterest = (jobId: string) => {
        console.log(`User expressed interest in Job ID: ${jobId}. Backend MeshNode handles actual P2P message.`);
        // Example: alert(`Interest expressed for ${jobId}. (Conceptual UI action)`);
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
                                <TableCell className="font-mono text-xs" title={job.originator_did}>{job.originator_did.substring(0, 25)}...</TableCell>
                                <TableCell className="font-mono text-xs" title={job.params.wasm_cid}>{job.params.wasm_cid.substring(0, 20)}...</TableCell>
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