import type { ReactNode } from 'react';
import { IncomingCallModal } from '../components/IncomingCallModal';
import { SafeCallOverlay } from '../components/SafeCallOverlay';
import { ServerSidebar } from '../components/ServerSidebar';

interface AppShellProps {
    children: ReactNode;
    callOverlayResetKey: string;
}

export function AppShell({ children, callOverlayResetKey }: AppShellProps) {
    return (
        <div className="flex h-screen w-full bg-background text-white overflow-hidden font-sans">
            <IncomingCallModal />
            <SafeCallOverlay resetKey={callOverlayResetKey} />
            <ServerSidebar />
            {children}
        </div>
    );
}
