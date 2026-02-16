import { useAppStore } from './store';
import { AppUpdateNotice } from './components/AppUpdateNotice';
import { AuthScreen } from './components/AuthScreen';
import { AppShell } from './layouts/AppShell';
import { AppRoutes } from './app/AppRoutes';
import { useAppLifecycle } from './app/useAppLifecycle';

function App() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const activeCall = useAppStore((s) => s.activeCall);

    useAppLifecycle();

    const callOverlayResetKey = activeCall
        ? `${activeCall.status}:${activeCall.peerId ?? 'none'}`
        : 'none';

    return (
        <>
            <AppUpdateNotice />
            {!isAuthenticated ? (
                <AuthScreen />
            ) : (
                <AppShell callOverlayResetKey={callOverlayResetKey}>
                    <AppRoutes />
                </AppShell>
            )}
        </>
    );
}

export default App;
