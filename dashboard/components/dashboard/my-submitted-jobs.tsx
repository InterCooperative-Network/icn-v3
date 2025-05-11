import React, { useEffect, useState } from 'react';
import { MeshJob, JobReceiptLink } from '@/types/mesh'; // Assuming types are in @/types/mesh

// Mock shadcn/ui components - replace with actual imports
const Table = ({ children }: { children: React.ReactNode }) => <table className="w-full border-collapse text-sm">{children}</table>;
const TableHeader = ({ children }: { children: React.ReactNode }) => <thead className="bg-gray-50">{children}</thead>;
const TableRow = ({ children, className }: { children: React.ReactNode, className?: string }) => <tr className={`border-b ${className || ''}`}>{children}</tr>;
const TableHead = ({ children }: { children: React.ReactNode }) => <th className="px-4 py-2 text-left font-semibold text-gray-600">{children}</th>;
const TableBody = ({ children }: { children: React.ReactNode }) => <tbody>{children}</tbody>;
const TableCell = ({ children, className }: { children: React.ReactNode, className?: string }) => <td className={`px-4 py-3 ${className || ''}`}>{children}</td>;
const Button = ({ onClick, children, variant, size, className }: { onClick?: () => void, children: React.ReactNode, variant?: string, size?: string, className?: string }) => 
    <button onClick={onClick} className={`px-3 py-1 rounded text-xs ${variant === 'link' ? 'text-blue-500 hover:underline p-0' : 'bg-blue-500 text-white hover:bg-blue-600'} ${className || ''}`}>{children}</button>;
const Badge = ({ children, variant }: { children: React.ReactNode, variant?: string }) => 
    <span className={`px-2 py-0.5 text-xs font-semibold rounded-full ${variant === 'success' ? 'bg-green-100 text-green-700' : variant === 'pending' ? 'bg-yellow-100 text-yellow-700' : 'bg-gray-100 text-gray-700'}`}>{children}</span>;
// End mock shadcn/ui components

interface MySubmittedJobsProps {
    onViewReceipt: (receiptCid: string) => void;
}

// Mock API fetcher functions
const fetchOriginatedJobs = async (): Promise<MeshJob[]> => {
    console.log('Fetching originated jobs...');
    return new Promise(resolve => {
        setTimeout(() => {
            resolve([
                {
                    job_id: 'job_orig_001',
                    originator_did: 'did:icn:localnode',
                    params: {
                        wasm_cid: 'bafybeiaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                        function_name: 'process_data',
                        required_resources_json: '{"cpu": 1, "memory_mb": 256}',
                        qos_profile: 'Balanced',
                    },
                    submitted_at: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(), // 2 hours ago
                },
                {
                    job_id: 'job_orig_002',
                    originator_did: 'did:icn:localnode',
                    params: {
                        wasm_cid: 'bafybeibbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                        function_name: 'generate_report',
                        required_resources_json: '{"cpu": 2, "memory_mb": 512}',
                        qos_profile: 'Fast',
                    },
                    submitted_at: new Date(Date.now() - 5 * 60 * 60 * 1000).toISOString(), // 5 hours ago
                },
                 {
                    job_id: 'job_orig_003_no_receipt',
                    originator_did: 'did:icn:localnode',
                    params: {
                        wasm_cid: 'bafybeiccccccccccccccccccccccccccccccccccc',
                        function_name: 'analyze_stream',
                        required_resources_json: '{"cpu": 1, "memory_mb": 128}',
                        qos_profile: 'Cheap',
                    },
                    submitted_at: new Date(Date.now() - 10 * 60 * 1000).toISOString(), // 10 mins ago
                },
            ]);
        }, 800);
    });
};

const fetchJobReceiptLink = async (jobId: string): Promise<JobReceiptLink> => {
    console.log(`Fetching receipt link for job ID: ${jobId}`);
    return new Promise(resolve => {
        setTimeout(() => {
            if (jobId === 'job_orig_001') {
                resolve({ job_id: jobId, receipt_cid: 'bafyreceipt001examplecid' });
            } else if (jobId === 'job_orig_002') {
                resolve({ job_id: jobId, receipt_cid: 'bafyreceipt002examplecid' });
            } else {
                resolve({ job_id: jobId, receipt_cid: undefined });
            }
        }, 500);
    });
};

export function MySubmittedJobs({ onViewReceipt }: MySubmittedJobsProps) {
    const [jobs, setJobs] = useState<MeshJob[]>([]);
    const [receiptLinks, setReceiptLinks] = useState<Record<string, JobReceiptLink>>({});
    const [isLoadingJobs, setIsLoadingJobs] = useState<boolean>(true);
    const [jobsError, setJobsError] = useState<string | null>(null);
    // Individual loading/error states for receipt links could be added if needed

    useEffect(() => {
        setIsLoadingJobs(true);
        fetchOriginatedJobs()
            .then(fetchedJobs => {
                setJobs(fetchedJobs);
                // Fetch receipt links for all jobs
                fetchedJobs.forEach(job => {
                    fetchJobReceiptLink(job.job_id)
                        .then(link => {
                            setReceiptLinks((prev: Record<string, JobReceiptLink>) => ({ ...prev, [job.job_id]: link }));
                        })
                        .catch(err => console.error(`Failed to fetch receipt link for ${job.job_id}:`, err));
                });
            })
            .catch(err => {
                setJobsError(err.message || 'Failed to load your jobs.');
            })
            .finally(() => {
                setIsLoadingJobs(false);
            });
    }, []);

    const getJobStatus = (jobId: string): { text: string; variant: string } => {
        if (receiptLinks[jobId]?.receipt_cid) {
            return { text: 'Completed', variant: 'success' };
        }
        // More sophisticated status logic can be added here later
        return { text: 'Pending', variant: 'pending' };
    };

    if (isLoadingJobs) return <p>Loading your submitted jobs...</p>;
    if (jobsError) return <p className="text-red-500">Error: {jobsError}</p>;
    if (jobs.length === 0) return <p>You have not submitted any jobs yet.</p>;

    return (
        <section>
            <h2 className="text-xl font-semibold mb-4">My Submitted Jobs</h2>
            <div className="overflow-x-auto">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>Job ID</TableHead>
                            <TableHead>WASM CID</TableHead>
                            <TableHead>Submitted</TableHead>
                            <TableHead>Status</TableHead>
                            <TableHead>Receipt CID</TableHead>
                            {/* <TableHead>Actions</TableHead> */}
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {jobs.map((job: MeshJob) => {
                            const status = getJobStatus(job.job_id);
                            const receiptCid = receiptLinks[job.job_id]?.receipt_cid;
                            return (
                                <TableRow key={job.job_id}>
                                    <TableCell className="font-mono text-xs">{job.job_id}</TableCell>
                                    <TableCell className="font-mono text-xs">{job.params.wasm_cid.substring(0, 20)}...</TableCell>
                                    <TableCell>{new Date(job.submitted_at).toLocaleString()}</TableCell>
                                    <TableCell><Badge variant={status.variant}>{status.text}</Badge></TableCell>
                                    <TableCell>
                                        {receiptCid ? (
                                            <Button 
                                                variant="link" 
                                                size="sm" 
                                                onClick={() => onViewReceipt(receiptCid)}
                                                className="p-0 h-auto text-xs"
                                            >
                                                {receiptCid.substring(0,20)}...
                                            </Button>
                                        ) : (
                                            <span className="text-gray-500 text-xs">N/A</span>
                                        )}
                                    </TableCell>
                                    {/* <TableCell>
                                        {/* Placeholder for View Interests button */}
                                        {/* <Button variant="outline" size="sm">View Interests</Button> */}
                                    {/* </TableCell> */}
                                </TableRow>
                            );
                        })}
                    </TableBody>
                </Table>
            </div>
        </section>
    );
} 