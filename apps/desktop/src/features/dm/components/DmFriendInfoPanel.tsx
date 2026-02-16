import { Calendar, Phone, Settings, UserCircle, Video } from 'lucide-react';
import type { Friend } from '../../../types';

interface DmFriendInfoPanelProps {
    friend: Friend;
    onJoinCall: (friendId: string) => Promise<void>;
}

export function DmFriendInfoPanel({ friend, onJoinCall }: DmFriendInfoPanelProps) {
    return (
        <div className="w-80 bg-surface border-l border-white/5 flex flex-col animate-slide-in">
            <div className="p-6 border-b border-white/5">
                <div className="flex flex-col items-center text-center">
                    <div className="w-24 h-24 rounded-full bg-gradient-to-br from-primary to-secondary mb-4 relative">
                        <div className="absolute bottom-1 right-1 w-6 h-6 bg-green-500 rounded-full border-4 border-surface" />
                    </div>
                    <h2 className="text-xl font-bold mb-1">{friend.username}</h2>
                    <p className="text-sm text-green-400 flex items-center gap-1">
                        <span className="w-2 h-2 bg-green-500 rounded-full" />
                        Online
                    </p>
                </div>
            </div>

            <div className="p-4 border-b border-white/5 space-y-2">
                <button
                    onClick={() => onJoinCall(friend.id)}
                    className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-green-600 hover:bg-green-500 text-white font-medium rounded-lg transition"
                >
                    <Phone className="w-5 h-5" />
                    Voice Call
                </button>

                <button
                    onClick={() => onJoinCall(friend.id)}
                    className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-blue-600 hover:bg-blue-500 text-white font-medium rounded-lg transition"
                    title="Video call (beta fallback to secure voice)"
                >
                    <Video className="w-5 h-5" />
                    Video Call (Beta)
                </button>
            </div>

            <div className="flex-1 overflow-y-auto p-4 space-y-4">
                <div className="bg-black/20 rounded-lg p-4">
                    <h3 className="text-sm font-bold text-gray-400 uppercase mb-3">About</h3>
                    <div className="space-y-3">
                        <div className="flex items-center gap-3 text-sm">
                            <Calendar className="w-4 h-4 text-gray-400" />
                            <div>
                                <div className="text-gray-400 text-xs">Member since</div>
                                <div className="text-white">
                                    {friend.last_seen ? new Date(friend.last_seen).toLocaleDateString() : 'Recently'}
                                </div>
                            </div>
                        </div>

                        <div className="flex items-center gap-3 text-sm">
                            <UserCircle className="w-4 h-4 text-gray-400" />
                            <div>
                                <div className="text-gray-400 text-xs">User ID</div>
                                <div className="text-white text-xs font-mono">{friend.id.slice(0, 8)}...</div>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="bg-black/20 rounded-lg p-4">
                    <h3 className="text-sm font-bold text-gray-400 uppercase mb-3">Security</h3>
                    <div className="flex items-center gap-2 text-green-400 text-sm">
                        <span>🔒</span>
                        <span>End-to-end encrypted</span>
                    </div>
                    <p className="text-xs text-gray-500 mt-2">
                        Messages are encrypted on your device. Only you and {friend.username} can read them.
                    </p>
                </div>

                <div className="bg-black/20 rounded-lg p-4">
                    <h3 className="text-sm font-bold text-gray-400 uppercase mb-3">Quick Commands</h3>
                    <div className="space-y-2 text-xs">
                        <div className="flex justify-between">
                            <span className="text-primary font-mono">/clear</span>
                            <span className="text-gray-500">Clear local view</span>
                        </div>
                        <div className="flex justify-between">
                            <span className="text-primary font-mono">/deleteall</span>
                            <span className="text-gray-500">Delete all messages</span>
                        </div>
                        <div className="flex justify-between">
                            <span className="text-primary font-mono">/shrug</span>
                            <span className="text-gray-500">¯\_(ツ)_/¯</span>
                        </div>
                    </div>
                </div>
            </div>

            <div className="p-4 border-t border-white/5">
                <button className="w-full flex items-center justify-center gap-2 px-4 py-2 hover:bg-white/5 rounded-lg transition text-gray-400">
                    <Settings className="w-4 h-4" />
                    <span className="text-sm">More Options</span>
                </button>
            </div>
        </div>
    );
}
