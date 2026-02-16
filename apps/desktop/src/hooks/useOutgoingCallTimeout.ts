import { useEffect } from 'react';
import { useAppStore } from '../store';

export function useOutgoingCallTimeout() {
    const activeCall = useAppStore((s) => s.activeCall);
    const cancelOutgoingCall = useAppStore((s) => s.cancelOutgoingCall);

    useEffect(() => {
        if (activeCall?.status !== 'calling') {
            return;
        }

        const timeout = window.setTimeout(() => {
            console.warn('[Call] Outgoing call timed out, cancelling...');
            void cancelOutgoingCall();
        }, 30_000);

        return () => {
            window.clearTimeout(timeout);
        };
    }, [activeCall?.status, cancelOutgoingCall]);
}
