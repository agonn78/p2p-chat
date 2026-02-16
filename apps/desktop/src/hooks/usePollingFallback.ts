import { useEffect } from 'react';
import { POLL_INTERVAL_MS, useAppStore } from '../store';

export function usePollingFallback() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const activeRoom = useAppStore((s) => s.activeRoom);
    const wsConnected = useAppStore((s) => s.wsConnected);
    const pollForNewMessages = useAppStore((s) => s.pollForNewMessages);

    useEffect(() => {
        if (!isAuthenticated || !activeRoom) {
            return;
        }

        console.log(`[App] 📊 Starting polling (interval: ${POLL_INTERVAL_MS}ms)`);

        const pollInterval = window.setInterval(() => {
            if (!wsConnected) {
                console.log('[App] 📬 Polling for messages (WS disconnected)...');
            }
            void pollForNewMessages();
        }, POLL_INTERVAL_MS);

        return () => {
            console.log('[App] 📊 Stopping polling');
            window.clearInterval(pollInterval);
        };
    }, [isAuthenticated, activeRoom, wsConnected, pollForNewMessages]);
}
