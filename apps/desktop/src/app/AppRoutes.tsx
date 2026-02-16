import { useState } from 'react';
import { useAppStore } from '../store';
import { DmPage } from '../pages/DmPage';
import { ServerPage } from '../pages/ServerPage';

export function AppRoutes() {
    const activeServer = useAppStore((s) => s.activeServer);
    const [activeDM, setActiveDM] = useState<string | null>(null);
    const [showAddFriend, setShowAddFriend] = useState(false);
    const [isMuted, setIsMuted] = useState(false);

    return (
        <>
            <div className={activeServer ? 'hidden' : 'contents'}>
                <DmPage
                    activeDM={activeDM}
                    setActiveDM={setActiveDM}
                    showAddFriend={showAddFriend}
                    setShowAddFriend={setShowAddFriend}
                    isMuted={isMuted}
                    setIsMuted={setIsMuted}
                />
            </div>
            {activeServer && <ServerPage />}
        </>
    );
}
