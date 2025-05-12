'use client';

import React, { useEffect, useState, useMemo } from 'react';
import { fetchReputationProfiles, ReputationProfileSummary } from '@/lib/api';
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"; // Assuming ShadCN UI table path
import { ArrowUpDown } from "lucide-react";
import { Button } from "@/components/ui/button"; // For sortable headers

// Helper to format time (simplified)
const formatTimeAgo = (timestamp: number | null): string => {
    if (timestamp === null) return 'N/A';
    const now = new Date();
    const date = new Date(timestamp * 1000); // Assuming timestamp is in seconds
    const diffSeconds = Math.round((now.getTime() - date.getTime()) / 1000);

    if (diffSeconds < 60) return `${diffSeconds}s ago`;
    const diffMinutes = Math.round(diffSeconds / 60);
    if (diffMinutes < 60) return `${diffMinutes}m ago`;
    const diffHours = Math.round(diffMinutes / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.round(diffHours / 24);
    return `${diffDays}d ago`;
};

type SortKey = keyof ReputationProfileSummary | null;

export function ReputationLeaderboard() {
    const [profiles, setProfiles] = useState<ReputationProfileSummary[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [sortKey, setSortKey] = useState<SortKey>('score');
    const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');

    useEffect(() => {
        async function loadProfiles() {
            try {
                setLoading(true);
                const data = await fetchReputationProfiles();
                setProfiles(data);
            } catch (e) {
                setError(e instanceof Error ? e.message : 'Unknown error fetching profiles');
            } finally {
                setLoading(false);
            }
        }
        loadProfiles();
    }, []);

    const sortedProfiles = useMemo(() => {
        if (!sortKey) return profiles;
        return [...profiles].sort((a, b) => {
            const valA = a[sortKey];
            const valB = b[sortKey];

            let comparison = 0;
            if (typeof valA === 'number' && typeof valB === 'number') {
                comparison = valA - valB;
            } else if (typeof valA === 'string' && typeof valB === 'string') {
                comparison = valA.localeCompare(valB);
            } else if (valA === null && valB !== null) {
                comparison = -1; 
            } else if (valA !== null && valB === null) {
                comparison = 1;
            }
            
            return sortOrder === 'asc' ? comparison : -comparison;
        });
    }, [profiles, sortKey, sortOrder]);

    const handleSort = (key: SortKey) => {
        if (sortKey === key) {
            setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc');
        } else {
            setSortKey(key);
            setSortOrder('desc'); // Default to descending for new column
        }
    };

    if (loading) return <div className="p-4 text-center">Loading reputation profiles...</div>;
    if (error) return <div className="p-4 text-center text-red-500">Error: {error}</div>;
    if (profiles.length === 0) return <div className="p-4 text-center">No reputation profiles found.</div>;

    const renderSortableHeader = (key: SortKey, label: string) => (
        <TableHead className="cursor-pointer" onClick={() => handleSort(key)}>
            <Button variant="ghost" size="sm" className="px-2 py-1">
                {label}
                {sortKey === key && (sortOrder === 'asc' ? <ArrowUpDown className="ml-2 h-3 w-3 rotate-180" /> : <ArrowUpDown className="ml-2 h-3 w-3" />)}
                {sortKey !== key && <ArrowUpDown className="ml-2 h-3 w-3 opacity-30" />}
            </Button>
        </TableHead>
    );

    return (
        <div className="container mx-auto py-6">
            <h2 className="text-2xl font-semibold mb-4">Reputation Leaderboard</h2>
            <div className="rounded-md border">
                <Table>
                    <TableHeader>
                        <TableRow>
                            {renderSortableHeader('subject_did', 'Executor DID')}
                            {renderSortableHeader('score', 'Score')}
                            {renderSortableHeader('successful_jobs', 'Successful Jobs')}
                            {renderSortableHeader('failed_jobs', 'Failed Jobs')}
                            {renderSortableHeader('last_updated', 'Last Updated')}
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {sortedProfiles.map((profile) => (
                            <TableRow key={profile.subject_did}>
                                <TableCell className="font-medium truncate max-w-xs" title={profile.subject_did}>{profile.subject_did}</TableCell>
                                <TableCell className="text-center">{profile.score.toFixed(1)}</TableCell>
                                <TableCell className="text-center">{profile.successful_jobs}</TableCell>
                                <TableCell className="text-center">{profile.failed_jobs}</TableCell>
                                <TableCell className="text-right">{formatTimeAgo(profile.last_updated)}</TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </div>
    );
}

export default ReputationLeaderboard; 