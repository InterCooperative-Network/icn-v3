import React, { useEffect, useState, useCallback } from 'react';
import { MeshJob, JobReceiptLink } from '@/types/mesh';

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

export function MySubmittedJobs({ onViewReceipt }: MySubmittedJobsProps) {
    const [jobs, setJobs] = useState<MeshJob[]>([]);
    const [receiptLinks, setReceiptLinks] = useState<Record<string, JobReceiptLink>>({});
    const [isLoadingJobs, setIsLoadingJobs] = useState<boolean>(true);
    const [jobsError, setJobsError] = useState<string | null>(null);
    // For simplicity, individual errors for receipt links are logged to console but not displayed per row.

    const fetchJobReceiptLinkCallback = useCallback(async (jobId: string) => {
        try {
            const response = await fetch(`/api/v1/mesh/jobs/${jobId}/receipt_cid`);
            if (!response.ok) {
                // It's common for this to 404 if no receipt exists, so don't treat as a full error for UI
                if (response.status === 404) {
                    console.log(`No receipt link found for job ${jobId} (404).`);
                    setReceiptLinks(prev => ({ ...prev, [jobId]: { job_id: jobId, receipt_cid: undefined } }));
                    return; 
                }
                const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                throw new Error(errorData.message || `HTTP error fetching receipt link! status: ${response.status}`);
            }
            const data: JobReceiptLink = await response.json();
            setReceiptLinks(prev => ({ ...prev, [jobId]: data }));
        } catch (e: any) {
            console.error(`Failed to fetch receipt link for ${jobId}:`, e.message);
            // Optionally set a specific error state for this link if needed for UI
        }
    }, []);

    useEffect(() => {
        const fetchOriginatedJobs = async () => {
            setIsLoadingJobs(true);
            setJobsError(null);
            try {
                const response = await fetch('/api/v1/mesh/jobs/originated');
                if (!response.ok) {
                    const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                    throw new Error(errorData.message || `HTTP error! status: ${response.status}`);
                }
                const fetchedJobs: MeshJob[] = await response.json();
                setJobs(fetchedJobs);

                // After fetching jobs, fetch their receipt links
                // Using Promise.all to fetch them concurrently for better performance
                await Promise.all(fetchedJobs.map(job => fetchJobReceiptLinkCallback(job.job_id)));
                
            } catch (e: any) {
                setJobsError(e.message || 'Failed to load your jobs.');
            } finally {
                setIsLoadingJobs(false);
            }
        };

        fetchOriginatedJobs();
    }, [fetchJobReceiptLinkCallback]);

    const getJobStatus = (jobId: string): { text: string; variant: string } => {
        if (receiptLinks[jobId]?.receipt_cid) {
            return { text: 'Completed', variant: 'success' };
        }
        return { text: 'Pending', variant: 'pending' };
    };

    if (isLoadingJobs && jobs.length === 0) return <p>Loading your submitted jobs...</p>; // Show loading only if no jobs displayed yet
    if (jobsError) return <p className="text-red-500">Error: {jobsError}</p>;
    if (jobs.length === 0 && !isLoadingJobs) return <p>You have not submitted any jobs yet.</p>;

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
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {jobs.map((job: MeshJob) => {
                            const status = getJobStatus(job.job_id);
                            const receiptCid = receiptLinks[job.job_id]?.receipt_cid;
                            // Row-specific loading for receipt_cid could be added here if fetchJobReceiptLinkCallback set individual loading states
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
                                        ) : receiptLinks[job.job_id] === undefined ? (
                                            <span className="text-gray-400 text-xs">Loading...</span>
                                        ) : (
                                            <span className="text-gray-500 text-xs">N/A</span>
                                        )}
                                    </TableCell>
                                </TableRow>
                            );
                        })}
                    </TableBody>
                </Table>
            </div>
            {isLoadingJobs && jobs.length > 0 && <p className="text-sm text-gray-500 mt-2">Updating job details...</p>}
        </section>
    );
} 