import React, { useState } from 'react';
import { MySubmittedJobs } from './my-submitted-jobs';
import { ExecutionReceiptModal } from './execution-receipt-modal';

// Placeholder components for sections to be implemented later
const AvailableMeshJobsPlaceholder = () => (
    <section>
        <h2 className="text-xl font-semibold mb-4 mt-8">Available Jobs on the Mesh (Placeholder)</h2>
        <p className="text-gray-500">Implementation pending: This section will display jobs available for execution from other nodes on the mesh.</p>
    </section>
);

const RecentReceiptAnnouncementsPlaceholder = () => (
    <section>
        <h2 className="text-xl font-semibold mb-4 mt-8">Recent Execution Receipt Announcements (Placeholder)</h2>
        <p className="text-gray-500">Implementation pending: This section will display recent receipt announcements from the mesh network.</p>
    </section>
);

export function MeshJobsOverview() {
    const [selectedReceiptCid, setSelectedReceiptCid] = useState<string | null>(null);

    const handleViewReceipt = (cid: string) => {
        setSelectedReceiptCid(cid);
    };

    const handleCloseModal = () => {
        setSelectedReceiptCid(null);
    };

    return (
        <div className="p-4 md:p-6 space-y-6">
            <header>
                <h1 className="text-2xl md:text-3xl font-bold text-gray-800">Mesh Compute Dashboard</h1>
                <p className="text-sm text-gray-600 mt-1">Oversee your submitted jobs, discover available tasks, and view execution receipts from the ICN mesh network.</p>
            </header>

            <MySubmittedJobs onViewReceipt={handleViewReceipt} />
            
            <AvailableMeshJobsPlaceholder />
            
            <RecentReceiptAnnouncementsPlaceholder />

            {/* Render the modal conditionally */}
            <ExecutionReceiptModal
                receiptCid={selectedReceiptCid} // Will be null if no receipt is selected
                isOpen={!!selectedReceiptCid} // Boolean based on whether a CID is selected
                onClose={handleCloseModal}
            />
        </div>
    );
} 