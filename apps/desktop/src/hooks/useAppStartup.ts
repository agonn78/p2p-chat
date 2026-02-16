import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../store';

export function useAppStartup() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const fetchFriends = useAppStore((s) => s.fetchFriends);
    const fetchServers = useAppStore((s) => s.fetchServers);

    useEffect(() => {
        if (!isAuthenticated) {
            return;
        }

        fetchFriends();
        fetchServers();
        invoke<number>('api_drain_outbox', { limit: 200 }).catch(() => undefined);
    }, [isAuthenticated, fetchFriends, fetchServers]);
}
