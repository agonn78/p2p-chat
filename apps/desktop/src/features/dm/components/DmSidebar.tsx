import { LogOut, Mic, MicOff, UserPlus, Wifi, WifiOff } from 'lucide-react';
import type { Friend, User } from '../../../types';

interface DmSidebarProps {
    friends: Friend[];
    activeDM: string | null;
    onlineFriends: string[];
    wsConnected: boolean;
    user: User | null;
    isMuted: boolean;
    onToggleMute: () => void;
    onShowAddFriend: () => void;
    onFriendClick: (friendId: string) => void;
    onLogout: () => void;
    getUnreadCount: (friendId: string) => number;
}

export function DmSidebar({
    friends,
    activeDM,
    onlineFriends,
    wsConnected,
    user,
    isMuted,
    onToggleMute,
    onShowAddFriend,
    onFriendClick,
    onLogout,
    getUnreadCount,
}: DmSidebarProps) {
    return (
        <div className="w-72 bg-surface flex flex-col border-r border-white/5 relative z-10 glass-panel">
            <div className="p-4 border-b border-white/5 flex items-center justify-between">
                <h1 className="text-xl font-bold bg-gradient-to-r from-primary to-secondary bg-clip-text text-transparent">
                    P2P Nitro
                </h1>

                <div className="flex items-center gap-2">
                    <div
                        className={`p-1.5 rounded ${wsConnected ? 'text-green-400' : 'text-yellow-400'}`}
                        title={wsConnected ? 'Real-time connected' : 'Polling mode (5s)'}
                    >
                        {wsConnected ? <Wifi className="w-4 h-4" /> : <WifiOff className="w-4 h-4" />}
                    </div>

                    <button
                        onClick={onShowAddFriend}
                        className="p-2 hover:bg-white/10 rounded-lg transition"
                        title="Add Friend"
                    >
                        <UserPlus className="w-5 h-5 text-gray-400" />
                    </button>
                </div>
            </div>

            <div className="flex-1 overflow-y-auto p-2 space-y-1">
                {friends.map((friend) => {
                    const unreadCount = getUnreadCount(friend.id);
                    const isActive = activeDM === friend.id;
                    const isOnline = onlineFriends.includes(friend.id);

                    return (
                        <button
                            key={friend.id}
                            onClick={() => onFriendClick(friend.id)}
                            className={`w-full flex items-center gap-3 p-3 rounded-lg transition relative ${isActive
                                ? 'bg-primary/20 border border-primary/30'
                                : 'hover:bg-white/5'
                                }`}
                        >
                            <div className="relative flex-shrink-0">
                                <div className="w-12 h-12 rounded-full bg-gradient-to-br from-primary to-secondary" />
                                <div className={`absolute bottom-0 right-0 w-3.5 h-3.5 rounded-full border-2 border-surface ${isOnline ? 'bg-green-500' : 'bg-gray-500'}`} />
                            </div>

                            <div className="flex-1 text-left">
                                <div className="font-medium text-sm">{friend.username}</div>
                                <div className={`text-xs ${isOnline ? 'text-green-400' : 'text-gray-500'}`}>
                                    {isOnline ? 'Online' : 'Offline'}
                                </div>
                            </div>

                            {unreadCount > 0 && (
                                <div className="bg-red-500 text-white text-xs font-bold rounded-full min-w-[20px] h-5 flex items-center justify-center px-1.5">
                                    {unreadCount > 99 ? '99+' : unreadCount}
                                </div>
                            )}
                        </button>
                    );
                })}
            </div>

            <div className="p-3 bg-black/20 backdrop-blur-md border-t border-white/5">
                <div className="flex items-center justify-between">
                    <div className="flex items-center">
                        <div className="w-8 h-8 rounded-full bg-gradient-to-tr from-primary to-secondary" />
                        <div className="ml-2">
                            <div className="text-sm font-medium">{user?.username}</div>
                            <div className="text-xs text-green-400">Online</div>
                        </div>
                    </div>

                    <div className="flex space-x-1">
                        <button
                            onClick={onToggleMute}
                            className={`p-1.5 rounded ${isMuted ? 'bg-red-500/20 text-red-400' : 'hover:bg-white/10'}`}
                        >
                            {isMuted ? <MicOff className="w-4 h-4" /> : <Mic className="w-4 h-4" />}
                        </button>

                        <button onClick={onLogout} className="p-1.5 hover:bg-white/10 rounded text-gray-400">
                            <LogOut className="w-4 h-4" />
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
