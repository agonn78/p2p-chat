const MESSAGE_STATUS_RANK: Record<string, number> = {
    sending: 0,
    failed: 0,
    sent: 1,
    delivered: 2,
    read: 3,
};

export const shouldPromoteStatus = (current?: string, next?: string): boolean => {
    if (!next) return false;
    const currentRank = MESSAGE_STATUS_RANK[current || 'sending'] ?? 0;
    const nextRank = MESSAGE_STATUS_RANK[next] ?? 0;
    return nextRank >= currentRank;
};
