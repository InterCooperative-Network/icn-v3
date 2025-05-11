import React, { useEffect, useState } from 'react';
import { ExecutionReceipt } from '@/types/mesh'; // Assuming types are in @/types/mesh

// Mock shadcn/ui components - replace with actual imports
const Dialog = ({ open, onOpenChange, children }: { open: boolean, onOpenChange: (open: boolean) => void, children: React.ReactNode }) => 
    open ? <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center p-4 z-50" onClick={() => onOpenChange(false)}>{children}</div> : null;
const DialogContent = ({ children, className }: { children: React.ReactNode, className?: string }) => 
    <div className={`bg-white p-6 rounded-lg shadow-xl max-w-lg w-full ${className || ''}`} onClick={(e) => e.stopPropagation()}>{children}</div>;
const DialogHeader = ({ children }: { children: React.ReactNode }) => <div className="mb-4">{children}</div>;
const DialogTitle = ({ children }: { children: React.ReactNode }) => <h2 className="text-lg font-semibold">{children}</h2>;
const DialogDescription = ({ children }: { children: React.ReactNode }) => <p className="text-sm text-gray-500">{children}</p>;
const DialogFooter = ({ children }: { children: React.ReactNode }) => <div className="mt-6 flex justify-end space-x-2">{children}</div>;
const Button = ({ onClick, children, variant }: { onClick?: () => void, children: React.ReactNode, variant?: string }) => 
    <button onClick={onClick} className={`px-4 py-2 rounded ${variant === 'outline' ? 'border border-gray-300' : 'bg-blue-500 text-white hover:bg-blue-600'}`}>{children}</button>;
// End mock shadcn/ui components

interface ExecutionReceiptModalProps {
    receiptCid: string | null;
    isOpen: boolean;
    onClose: () => void;
}

export function ExecutionReceiptModal({ receiptCid, isOpen, onClose }: ExecutionReceiptModalProps) {
    const [receipt, setReceipt] = useState<ExecutionReceipt | null>(null);
    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (isOpen && receiptCid) {
            const fetchReceiptDetails = async () => {
                setIsLoading(true);
                setError(null);
                setReceipt(null); // Clear previous receipt
                try {
                    const response = await fetch(`/api/v1/mesh/receipts/${receiptCid}`);
                    if (!response.ok) {
                        const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                        throw new Error(errorData.message || `HTTP error! status: ${response.status}`);
                    }
                    const data: ExecutionReceipt = await response.json();
                    setReceipt(data);
                } catch (e: any) {
                    setError(e.message || 'Failed to fetch receipt details.');
                } finally {
                    setIsLoading(false);
                }
            };

            fetchReceiptDetails();
        } else {
            // Reset when modal is closed or no CID
            setReceipt(null);
            setIsLoading(false);
            setError(null);
        }
    }, [isOpen, receiptCid]); // Dependency array ensures this runs when isOpen or receiptCid changes

    if (!isOpen) return null;

    return (
        <Dialog open={isOpen} onOpenChange={(openState) => !openState && onClose()}>
            <DialogContent className="sm:max-w-md md:max-w-lg">
                <DialogHeader>
                    <DialogTitle>Execution Receipt: {receiptCid || 'N/A'}</DialogTitle>
                </DialogHeader>
                <div className="mt-4 space-y-3 text-sm max-h-[60vh] overflow-y-auto pr-2">
                    {isLoading && <p>Loading receipt details...</p>}
                    {error && <p className="text-red-500">Error: {error}</p>}
                    {receipt && !isLoading && !error && (
                        <>
                            <p><strong>Job ID:</strong> {receipt.job_id}</p>
                            <p><strong>Executor DID:</strong> {receipt.executor}</p>
                            <p><strong>Status:</strong> <span className={`font-semibold ${receipt.status === 'CompletedSuccess' ? 'text-green-600' : 'text-red-600'}`}>{receipt.status}</span></p>
                            <p><strong>Result Data CID:</strong> {receipt.result_data_cid || 'N/A'}</p>
                            <p><strong>Logs CID:</strong> {receipt.logs_cid || 'N/A'}</p>
                            <div>
                                <p><strong>Resource Usage:</strong></p>
                                <ul className="list-disc list-inside pl-4 mt-1 space-y-1">
                                    {Object.entries(receipt.resource_usage).map(([key, value]) => (
                                        <li key={key}><span className="font-medium">{key.replace(/_/g, ' ')}:</span> {value.toString()}</li>
                                    ))}
                                </ul>
                            </div>
                            <p><strong>Execution Start Time:</strong> {new Date(receipt.execution_start_time * 1000).toLocaleString()}</p>
                            <p><strong>Execution End Time:</strong> {new Date(receipt.execution_end_time * 1000).toLocaleString()}</p>
                            <p><strong>Co-operative ID:</strong> {receipt.coop_id || 'N/A'}</p>
                            <p><strong>Community ID:</strong> {receipt.community_id || 'N/A'}</p>
                            <p className="break-all"><strong>Signature:</strong> {receipt.signature}</p>
                        </>
                    )}
                    {!receipt && !isLoading && !error && <p>No receipt details available for this CID, or receipt not found.</p>}
                </div>
                <DialogFooter>
                    <Button onClick={onClose} variant="outline">Close</Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
} 