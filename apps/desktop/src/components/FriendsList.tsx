import { useEffect } from 'react';
import { useAppStore } from '../store';
import { UserPlus, UserCheck, Users, Loader2 } from 'lucide-react';
import type { Friend } from '../types';

export function FriendsList() {
    const friends = useAppStore((s) => s.friends);
    const pendingRequests = useAppStore((s) => s.pendingRequests);
    const fetchFriends = useAppStore((s) => s.fetchFriends);
    const fetchPendingRequests = useAppStore((s) => s.fetchPendingRequests);
    const acceptFriend = useAppStore((s) => s.acceptFriend);
    const startCall = useAppStore((s) => s.startCall);

    useEffect(() => {
        fetchFriends();
        fetchPendingRequests();
    }, [fetchFriends, fetchPendingRequests]);

    return (
        <div className="flex flex-col h-full">
            {/* Pending Requests */}
            {pendingRequests.length > 0 && (
                <div className="p-3 border-b border-white/5">
                    <h3 className="text-xs font-semibold text-gray-500 uppercase mb-2 flex items-center gap-2">
                        <UserPlus className="w-3 h-3" />
                        Pending Requests ({pendingRequests.length})
                    </h3>
                    <div className="space-y-2">
                        {pendingRequests.map((req) => (
                            <div
                                key={req.id}
                                className="flex items-center justify-between bg-black/20 rounded-lg p-2"
                            >
                                <div className="flex items-center gap-2">
                                    <div className="w-8 h-8 rounded-full bg-gradient-to-br from-yellow-500 to-orange-500" />
                                    <span className="text-sm font-medium">{req.username}</span>
                                </div>
                                <button
                                    onClick={() => acceptFriend(req.id)}
                                    className="px-3 py-1 text-xs bg-green-600 hover:bg-green-500 rounded-md transition"
                                >
                                    Accept
                                </button>
                            </div>
                        ))}
                    </div>
                </div>
            )}

            {/* Friends List */}
            <div className="flex-1 overflow-y-auto p-3">
                <h3 className="text-xs font-semibold text-gray-500 uppercase mb-2 flex items-center gap-2">
                    <Users className="w-3 h-3" />
                    Friends ({friends.length})
                </h3>
                {friends.length === 0 ? (
                    <p className="text-sm text-gray-500 text-center py-8">No friends yet</p>
                ) : (
                    <div className="space-y-1">
                        {friends.map((friend) => (
                            <FriendItem key={friend.id} friend={friend} onCall={() => startCall(friend.id)} />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

function FriendItem({ friend, onCall }: { friend: Friend; onCall: () => void }) {
    const isOnline = friend.last_seen
        ? new Date(friend.last_seen).getTime() > Date.now() - 5 * 60 * 1000
        : false;

    return (
        <div className="group flex items-center justify-between p-2 rounded-lg hover:bg-white/5 transition cursor-pointer">
            <div className="flex items-center gap-3">
                <div className="relative">
                    <div className="w-10 h-10 rounded-full bg-gradient-to-br from-primary to-secondary" />
                    <div
                        className={`absolute bottom-0 right-0 w-3 h-3 rounded-full border-2 border-surface ${isOnline ? 'bg-green-500' : 'bg-gray-500'
                            }`}
                    />
                </div>
                <div>
                    <p className="font-medium text-sm">{friend.username}</p>
                    <p className={`text-xs ${isOnline ? 'text-green-400' : 'text-gray-500'}`}>
                        {isOnline ? 'Online' : 'Offline'}
                    </p>
                </div>
            </div>
            <div className="opacity-0 group-hover:opacity-100 transition">
                <button
                    onClick={onCall}
                    className="p-2 hover:bg-primary/20 rounded-lg text-primary transition"
                >
                    <UserCheck className="w-4 h-4" />
                </button>
            </div>
        </div>
    );
}

export function AddFriendModal({
    isOpen,
    onClose,
}: {
    isOpen: boolean;
    onClose: () => void;
}) {
    const sendFriendRequest = useAppStore((s) => s.sendFriendRequest);

    const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
        e.preventDefault();
        const formData = new FormData(e.currentTarget);
        const username = formData.get('username') as string;
        await sendFriendRequest(username);
        onClose();
    };

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50">
            <div className="glass-panel w-full max-w-md rounded-xl p-6 border border-white/10">
                <h2 className="text-xl font-bold mb-4">Add Friend</h2>
                <form onSubmit={handleSubmit}>
                    <input
                        name="username"
                        type="text"
                        placeholder="Enter username"
                        className="w-full px-4 py-3 bg-black/30 border border-white/10 rounded-lg outline-none focus:border-primary/50 transition mb-4"
                        required
                    />
                    <div className="flex gap-2">
                        <button
                            type="button"
                            onClick={onClose}
                            className="flex-1 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition"
                        >
                            Cancel
                        </button>
                        <button
                            type="submit"
                            className="flex-1 py-2 bg-primary hover:bg-primary/80 rounded-lg transition"
                        >
                            Send Request
                        </button>
                    </div>
                </form>
            </div>
        </div>
    );
}
