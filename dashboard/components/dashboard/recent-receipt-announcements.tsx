import React, { useEffect, useState } from 'react';
import { AnnouncedReceipt } from '@/types/mesh'; // Assuming types are in @/types/mesh

// Mock shadcn/ui components - replace with actual imports
const Table = ({ children }: { children: React.ReactNode }) => <table className="w-full border-collapse text-sm">{children}</table>;
const TableHeader = ({ children }: { children: React.ReactNode }) => <thead className="bg-gray-50">{children}</thead>;
const TableRow = ({ children, className }: { children: React.ReactNode, className?: string }) => <tr className={`border-b ${className || ''}`}>{children}</tr>;
const TableHead = ({ children }: { children: React.ReactNode }) => <th className="px-4 py-2 text-left font-semibold text-gray-600">{children}</th>;
const TableBody = ({ children }: { children: React.ReactNode }) => <tbody>{children}</tbody>;
const TableCell = ({ children, className }: { children: React.ReactNode, className?: string }) => <td className={`px-4 py-3 ${className || ''}`}>{children}</td>;
const Button = ({ onClick, children, variant, size, className }: { onClick?: () => void, children: React.ReactNode, variant?: string, size?: string, className?: string }) => 
    <button onClick={onClick} className={`px-3 py-1 rounded text-xs ${variant === 'link' ? 'text-blue-500 hover:underline p-0' : 'bg-blue-500 text-white hover:bg-blue-600'} ${className || ''}`}>{children}</button>;
// End mock shadcn/ui components

interface RecentReceiptAnnouncementsProps {
    onViewReceipt: (receiptCid: string) => void;
}

// Mock API fetcher function
const fetchAnnouncedReceipts = async (): Promise<AnnouncedReceipt[]> => {
    console.log('Fetching announced receipts...');
    return new Promise(resolve => {
        setTimeout(() => {
            resolve([
                {
                    job_id: 'job_orig_001',
                    executor_did: 'did:icn:executor_node_A:zyxw98765',
                    receipt_cid: 'bafyreceipt001examplecid',
                },
                {
                    job_id: 'job_avail_alpha_001',
                    executor_did: 'did:icn:executor_node_B:mlkj10987',
                    receipt_cid: 'bafyreceipt_alpha_1_example',
                },
                {
                    job_id: 'job_orig_002',
                    executor_did: 'did:icn:executor_node_C:fedc54321',
                    receipt_cid: 'bafyreceipt002examplecid',
                },
            ]);
        }, 900);
    });
};

export function RecentReceiptAnnouncements({ onViewReceipt }: RecentReceiptAnnouncementsProps) {
    const [announcements, setAnnouncements] = useState<AnnouncedReceipt[]>([]);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        setIsLoading(true);
        fetchAnnouncedReceipts()
            .then(fetchedAnnouncements => {
                setAnnouncements(fetchedAnnouncements);
            })
            .catch(err => {
                setError(err.message || 'Failed to load receipt announcements.');
            })
            .finally(() => {
                setIsLoading(false);
            });
    }, []);

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
                            <TableRow key={ann.receipt_cid}> {/* Assuming receipt_cid is unique for announcements list */}
                                <TableCell className="font-mono text-xs">{ann.job_id}</TableCell>
                                <TableCell className="font-mono text-xs">{ann.executor_did.substring(0, 25)}...</TableCell>
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