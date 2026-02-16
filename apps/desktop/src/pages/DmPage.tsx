import { useAppStore } from '../store';
import { AddFriendModal } from '../components/FriendsList';
import { DmSidebar } from '../features/dm/components/DmSidebar';
import { DmConversation } from '../features/dm/components/DmConversation';
import { DmNoSelectionState } from '../features/dm/components/DmNoSelectionState';
import { DmFriendInfoPanel } from '../features/dm/components/DmFriendInfoPanel';

interface DmPageProps {
    activeDM: string | null;
    setActiveDM: (value: string | null) => void;
    showAddFriend: boolean;
    setShowAddFriend: (value: boolean) => void;
    isMuted: boolean;
    setIsMuted: (value: boolean) => void;
}

export function DmPage({
    activeDM,
    setActiveDM,
    showAddFriend,
    setShowAddFriend,
    isMuted,
    setIsMuted,
}: DmPageProps) {
    const user = useAppStore((s) => s.user);
    const friends = useAppStore((s) => s.friends);
    const onlineFriends = useAppStore((s) => s.onlineFriends);
    const wsConnected = useAppStore((s) => s.wsConnected);
    const logout = useAppStore((s) => s.logout);
    const startCall = useAppStore((s) => s.startCall);
    const createOrGetDm = useAppStore((s) => s.createOrGetDm);
    const getUnreadCount = useAppStore((s) => s.getUnreadCount);

    const activeFriend = friends.find((friend) => friend.id === activeDM);

    const handleFriendClick = async (friendId: string) => {
        setActiveDM(friendId);
        await createOrGetDm(friendId);
    };

    const handleJoinCall = async (friendId: string) => {
        await startCall(friendId);
    };

    return (
        <>
            <DmSidebar
                friends={friends}
                activeDM={activeDM}
                onlineFriends={onlineFriends}
                wsConnected={wsConnected}
                user={user}
                isMuted={isMuted}
                onToggleMute={() => setIsMuted(!isMuted)}
                onShowAddFriend={() => setShowAddFriend(true)}
                onFriendClick={handleFriendClick}
                onLogout={logout}
                getUnreadCount={getUnreadCount}
            />

            <div className="flex-1 flex flex-col relative bg-background/50">
                {activeDM && activeFriend ? (
                    <DmConversation activeFriend={activeFriend} />
                ) : (
                    <DmNoSelectionState user={user} />
                )}
            </div>

            {activeFriend && (
                <DmFriendInfoPanel
                    friend={activeFriend}
                    onJoinCall={handleJoinCall}
                />
            )}

            <AddFriendModal isOpen={showAddFriend} onClose={() => setShowAddFriend(false)} />
        </>
    );
}
