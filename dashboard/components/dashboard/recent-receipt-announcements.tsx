import React, { useEffect, useState } from 'react';
import { AnnouncedReceipt } from '@/types/mesh';

// Mock shadcn/ui components - replace with actual imports
const Table = ({ children }: { children: React.ReactNode }) => <table className="w-full border-collapse text-sm">{children}</table>;
const TableHeader = ({ children }: { children: React.ReactNode }) => <thead className="bg-gray-50">{children}</thead>;
const TableRow = ({ children, className }: { children: React.ReactNode, className?: string }) => <tr className={`border-b ${className || ''}`}>{children}</tr>;
const TableHead = ({ children }: { children: React.ReactNode }) => <th className="px-4 py-2 text-left font-semibold text-gray-600">{children}</th>;
const TableBody = ({ children }: { children: React.ReactNode }) => <tbody>{children}</tbody>;
const TableCell = ({ children, className, ...rest }: { children: React.ReactNode, className?: string, [key: string]: any }) => <td className={`px-4 py-3 ${className || ''}`} {...rest}>{children}</td>;
const Button = ({ onClick, children, variant, size, className }: { onClick?: () => void, children: React.ReactNode, variant?: string, size?: string, className?: string }) => 
    <button onClick={onClick} className={`px-3 py-1 rounded text-xs ${variant === 'link' ? 'text-blue-500 hover:underline p-0' : 'bg-blue-500 text-white hover:bg-blue-600'} ${className || ''}`}>{children}</button>;
// End mock shadcn/ui components

interface RecentReceiptAnnouncementsProps {
    onViewReceipt: (receiptCid: string) => void;
}

export function RecentReceiptAnnouncements({ onViewReceipt }: RecentReceiptAnnouncementsProps) {
    const [announcements, setAnnouncements] = useState<AnnouncedReceipt[]>([]);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const fetchAnnouncements = async () => {
            setIsLoading(true);
            setError(null);
            try {
                const response = await fetch('/api/v1/mesh/receipts/announced');
                if (!response.ok) {
                    const errorData = await response.json().catch(() => ({ message: `HTTP error! status: ${response.status}` }));
                    throw new Error(errorData.message || `HTTP error! status: ${response.status}`);
                }
                const data: AnnouncedReceipt[] = await response.json();
                setAnnouncements(data);
            } catch (e: any) {
                setError(e.message || 'Failed to load receipt announcements.');
            } finally {
                setIsLoading(false);
            }
        };

        fetchAnnouncements();
    }, []); // Empty dependency array means this runs once on mount

    if (isLoading) return <p>Loading recent receipt announcements...</p>;
    if (error) return <p className="text-red-500">Error: {error}</p>;
    if (announcements.length === 0 && !isLoading) return <p>No receipt announcements discovered yet.</p>;

    return (
        <section>
            <h2 className="text-xl font-semibold mb-4">Recent Execution Receipt Announcements</h2>
            <div className="overflow-x-auto">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>Job ID</TableHead>
                            <TableHead>Executor DID</TableHead>
                            <TableHead>Receipt CID</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {announcements.map((ann: AnnouncedReceipt) => (
                            <TableRow key={ann.receipt_cid + ann.job_id}> {/* Combine for a more unique key if needed */}
                                <TableCell className="font-mono text-xs">{ann.job_id}</TableCell>
                                <TableCell className="font-mono text-xs" title={ann.executor_did}>{ann.executor_did.substring(0, 25)}...</TableCell>
                                <TableCell>
                                    <Button 
                                        variant="link"
                                        size="sm"
                                        onClick={() => onViewReceipt(ann.receipt_cid)}
                                        className="p-0 h-auto text-xs font-mono"
                                    >
                                        {ann.receipt_cid.substring(0, 20)}...
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