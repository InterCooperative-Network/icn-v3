import React, { useEffect, useState, useCallback } from 'react';
import { MeshJob, JobReceiptLink } from '@/types/mesh';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
// Potentially other components like Skeleton for loading states

// Mock shadcn/ui components - replace with actual imports
// const Table = ({ children }: { children: React.ReactNode }) => <table className="w-full border-collapse text-sm">{children}</table>;
// const TableHeader = ({ children }: { children: React.ReactNode }) => <thead className="bg-gray-50">{children}</thead>;
// const TableRow = ({ children, className }: { children: React.ReactNode, className?: string }) => <tr className={`border-b ${className || ''}`}>{children}</tr>;
// const TableHead = ({ children }: { children: React.ReactNode }) => <th className="px-4 py-2 text-left font-semibold text-gray-600">{children}</th>;
// const TableBody = ({ children }: { children: React.ReactNode }) => <tbody>{children}</tbody>;
// const TableCell = ({ children, className }: { children: React.ReactNode, className?: string }) => <td className={`px-4 py-3 ${className || ''}`}>{children}</td>;
// const Button = ({ onClick, children, variant, size, className }: { onClick?: () => void, children: React.ReactNode, variant?: string, size?: string, className?: string }) => 
//     <button onClick={onClick} className={`px-3 py-1 rounded text-xs ${variant === 'link' ? 'text-blue-500 hover:underline p-0' : 'bg-blue-500 text-white hover:bg-blue-600'} ${className || ''}`}>{children}</button>;
// const Badge = ({ children, variant }: { children: React.ReactNode, variant?: string }) => 
//     <span className={`px-2 py-0.5 text-xs font-semibold rounded-full ${variant === 'success' ? 'bg-green-100 text-green-700' : variant === 'pending' ? 'bg-yellow-100 text-yellow-700' : 'bg-gray-100 text-gray-700'}`}>{children}</span>;
// End mock shadcn/ui components

// Define an extended type for internal state management
interface InternalJobReceiptLink extends JobReceiptLink {
    isLoading: boolean;
    error?: string;
}

interface MySubmittedJobsProps {
    onViewReceipt: (receiptCid: string) => void;
}

export function MySubmittedJobs({ onViewReceipt }: MySubmittedJobsProps) {
    const [jobs, setJobs] = useState<MeshJob[]>([]);
    const [receiptLinks, setReceiptLinks] = useState<Record<string, InternalJobReceiptLink>>({});
    const [isLoadingJobs, setIsLoadingJobs] = useState<boolean>(true);
    const [jobsError, setJobsError] = useState<string | null>(null);
    // For simplicity, individual errors for receipt links are logged to console but not displayed per row.

    const fetchJobReceiptLinkCallback = useCallback(async (jobId: string) => {
        // Ensure the receipt link is marked as loading before the fetch
        setReceiptLinks(prev => ({ 
            ...prev, 
            [jobId]: { ...(prev[jobId] || { job_id: jobId, receipt_cid: undefined }), isLoading: true, error: undefined } as InternalJobReceiptLink 
        }));

        try {
            const response = await fetch(`/api/v1/mesh/jobs/${jobId}/receipt_cid`);
            if (!response.ok) {
                if (response.status === 404) {
                    console.log(`No receipt link found for job ${jobId} (404).`);
                    setReceiptLinks(prev => ({ 
                        ...prev, 
                        [jobId]: { ...(prev[jobId] || { job_id: jobId }), receipt_cid: undefined, isLoading: false } as InternalJobReceiptLink 
                    }));
                    return; 
                }
                const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                throw new Error(errorData.message || `HTTP error fetching receipt link! status: ${response.status}`);
            }
            const data: JobReceiptLink = await response.json();
            setReceiptLinks(prev => ({ 
                ...prev, 
                [jobId]: { ...data, isLoading: false } as InternalJobReceiptLink 
            }));
        } catch (e: any) {
            console.error(`Failed to fetch receipt link for ${jobId}:`, e.message);
            setReceiptLinks(prev => ({ 
                ...prev, 
                [jobId]: { ...(prev[jobId] || { job_id: jobId, receipt_cid: undefined }), isLoading: false, error: e.message } as InternalJobReceiptLink
            }));
        }
    }, []);

    useEffect(() => {
        const fetchOriginatedJobs = async () => {
            setIsLoadingJobs(true);
            setJobsError(null);
            setReceiptLinks({}); // Clear old links
            try {
                const response = await fetch('/api/v1/mesh/jobs/originated');
                if (!response.ok) {
                    const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                    throw new Error(errorData.message || `HTTP error! status: ${response.status}`);
                }
                const fetchedJobs: MeshJob[] = await response.json();
                setJobs(fetchedJobs);

                if (fetchedJobs.length > 0) {
                    const initialReceiptLinks = fetchedJobs.reduce((acc, job) => {
                        acc[job.job_id] = { job_id: job.job_id, receipt_cid: undefined, isLoading: true } as InternalJobReceiptLink;
                        return acc;
                    }, {} as Record<string, InternalJobReceiptLink>);
                    setReceiptLinks(initialReceiptLinks);

                    await Promise.all(fetchedJobs.map(job => fetchJobReceiptLinkCallback(job.job_id)));
                }
                
            } catch (e: any) {
                setJobsError(e.message || 'Failed to load your jobs.');
            } finally {
                setIsLoadingJobs(false);
            }
        };

        fetchOriginatedJobs();
    }, [fetchJobReceiptLinkCallback]);

    const getJobStatus = (jobId: string): { text: string; variant: "default" | "secondary" | "destructive" | "outline" | "success" | "warning" } => {
        const linkState = receiptLinks[jobId];
        if (linkState?.receipt_cid) {
            return { text: 'Completed', variant: 'success' };
        }
        if (linkState?.isLoading) {
            return { text: 'Loading...', variant: 'secondary' };
        }
        // Consider if linkState.error exists to show an error badge, e.g. 'Error', variant: 'destructive'
        return { text: 'Pending', variant: 'warning' };
    };

    return (
        <section>
            <h2 className="text-xl font-semibold mb-4">My Submitted Jobs</h2>
            <div className="rounded-md border">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead className="w-[200px]">Job ID</TableHead>
                            <TableHead>WASM CID</TableHead>
                            <TableHead>Submitted</TableHead>
                            <TableHead>Status</TableHead>
                            <TableHead className="text-right">Receipt CID</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {isLoadingJobs && jobs.length === 0 && (
                            <TableRow>
                                <TableCell colSpan={5} className="h-24 text-center">
                                    Loading your submitted jobs...
                                </TableCell>
                            </TableRow>
                        )}
                        {jobsError && (
                            <TableRow>
                                <TableCell colSpan={5} className="h-24 text-center text-red-500">
                                    Error: {jobsError}
                                </TableCell>
                            </TableRow>
                        )}
                        {!isLoadingJobs && !jobsError && jobs.length === 0 && (
                            <TableRow>
                                <TableCell colSpan={5} className="h-24 text-center">
                                    You have not submitted any jobs yet.
                                </TableCell>
                            </TableRow>
                        )}
                        {!isLoadingJobs && !jobsError && jobs.map((job: MeshJob) => {
                            const status = getJobStatus(job.job_id);
                            const receiptLink = receiptLinks[job.job_id]; // This is now InternalJobReceiptLink
                            const receiptCid = receiptLink?.receipt_cid;
                            
                            return (
                                <TableRow key={job.job_id}>
                                    <TableCell className="font-mono text-xs">{job.job_id}</TableCell>
                                    <TableCell className="font-mono text-xs truncate max-w-[150px]" title={job.params.wasm_cid}>
                                        {job.params.wasm_cid}
                                    </TableCell>
                                    <TableCell>{new Date(job.submitted_at).toLocaleString()}</TableCell>
                                    <TableCell>
                                        <Badge variant={status.variant as any}>{status.text}</Badge>
                                    </TableCell>
                                    <TableCell className="text-right">
                                        {receiptLink?.isLoading ? (
                                            <span className="text-gray-400 text-xs">Loading...</span>
                                        ) : receiptCid ? (
                                            <Button 
                                                variant="link" 
                                                size="sm" 
                                                onClick={() => onViewReceipt(receiptCid)}
                                                className="p-0 h-auto text-xs font-mono"
                                            >
                                                {receiptCid.substring(0,20)}...
                                            </Button>
                                        ) : receiptLink?.error ? (
                                            <span className="text-red-500 text-xs" title={receiptLink.error}>Error</span>
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