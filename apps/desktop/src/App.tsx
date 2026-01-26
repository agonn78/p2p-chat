import { useState, useEffect, useRef } from 'react';
import { Mic, Video, Settings, Hash, Send, UserPlus, Phone, LogOut, MicOff, UserCircle, Calendar, Wifi, WifiOff } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore, POLL_INTERVAL_MS } from './store';
import { AuthScreen } from './components/AuthScreen';
import { AddFriendModal } from './components/FriendsList';

function App() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const user = useAppStore((s) => s.user);
    const logout = useAppStore((s) => s.logout);
    const activeCall = useAppStore((s) => s.activeCall);
    const endCall = useAppStore((s) => s.endCall);
    const friends = useAppStore((s) => s.friends);
    const startCall = useAppStore((s) => s.startCall);
    const fetchFriends = useAppStore((s) => s.fetchFriends);

    // Chat Store
    const activeRoom = useAppStore((s) => s.activeRoom);
    const messages = useAppStore((s) => s.messages);
    const createOrGetDm = useAppStore((s) => s.createOrGetDm);
    const sendMessage = useAppStore((s) => s.sendMessage);
    const addMessage = useAppStore((s) => s.addMessage);
    const getUnreadCount = useAppStore((s) => s.getUnreadCount);
    const decryptMessageContent = useAppStore((s) => s.decryptMessageContent);
    const pollForNewMessages = useAppStore((s) => s.pollForNewMessages);

    // Connection state
    const wsConnected = useAppStore((s) => s.wsConnected);
    const setWsConnected = useAppStore((s) => s.setWsConnected);

    const [activeDM, setActiveDM] = useState<string | null>(null);
    const [msgInput, setMsgInput] = useState('');
    const [showAddFriend, setShowAddFriend] = useState(false);
    const [isMuted, setIsMuted] = useState(false);

    // Ref for auto-scrolling to bottom
    const messagesEndRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (isAuthenticated) {
            fetchFriends();
        }
    }, [isAuthenticated, fetchFriends]);

    // WebSocket Listener with connection tracking
    useEffect(() => {
        if (!isAuthenticated) {
            console.log('[App] Not authenticated, skipping WS listener setup');
            return;
        }

        console.log('[App] 🔌 Setting up WebSocket listener...');
        let lastMessageTime = Date.now();

        const setupListener = async () => {
            try {
                const unlisten = await listen<string>('ws-message', (event) => {
                    lastMessageTime = Date.now();

                    // Mark WebSocket as connected when we receive messages
                    if (!wsConnected) {
                        setWsConnected(true);
                    }

                    console.log('[App] 📨 WS Event received');
                    try {
                        const payload = JSON.parse(event.payload);

                        if (payload.type === 'NEW_MESSAGE') {
                            console.log('[App] ✉️ NEW_MESSAGE via WebSocket');
                            addMessage(payload.message);
                        }
                    } catch (e) {
                        console.error('[App] ❌ Failed to parse WS message:', e);
                    }
                });

                // Also listen for WS connection status
                const unlistenStatus = await listen<boolean>('ws-status', (event) => {
                    console.log('[App] 📡 WS Status changed:', event.payload);
                    setWsConnected(event.payload);
                });

                console.log('[App] ✅ WebSocket listeners attached');
                setWsConnected(true);

                return () => {
                    unlisten();
                    unlistenStatus();
                };
            } catch (error) {
                console.error('[App] ❌ Failed to setup WS listener:', error);
                setWsConnected(false);
                return () => { };
            }
        };

        let cleanup: (() => void) | null = null;

        setupListener().then(fn => {
            cleanup = fn;
        });

        return () => {
            console.log('[App] 🔌 Cleaning up WebSocket listener');
            if (cleanup) cleanup();
        };
    }, [isAuthenticated]);

    // Polling fallback - only active when WebSocket is disconnected OR as backup
    useEffect(() => {
        if (!isAuthenticated || !activeRoom) {
            return;
        }

        console.log(`[App] 📊 Starting polling (interval: ${POLL_INTERVAL_MS}ms, WS: ${wsConnected ? 'connected' : 'disconnected'})`);

        const pollInterval = setInterval(() => {
            // Always poll as backup, but log differently
            if (!wsConnected) {
                console.log('[App] 📬 Polling for messages (WS disconnected)...');
            }
            pollForNewMessages();
        }, POLL_INTERVAL_MS);

        return () => {
            console.log('[App] 📊 Stopping polling');
            clearInterval(pollInterval);
        };
    }, [isAuthenticated, activeRoom, wsConnected, pollForNewMessages]);

    // Auto-scroll to bottom when messages change
    useEffect(() => {
        if (messagesEndRef.current) {
            messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
        }
    }, [messages]);

    const handleJoinCall = async (friendId: string) => {
        startCall(friendId);
        try {
            const result = await invoke('join_call', { channel: friendId });
            console.log('Call started:', result);
        } catch (e) {
            console.error('Failed to start call:', e);
        }
    };

    const handleFriendClick = async (friendId: string) => {
        setActiveDM(friendId);
        await createOrGetDm(friendId);
    };

    const handleSendMessage = async () => {
        if (!msgInput.trim() || !activeRoom) return;
        try {
            await sendMessage(activeRoom, msgInput);
            setMsgInput('');
        } catch (e) {
            console.error('Failed to send messages:', e);
        }
    };

    const getSenderName = (senderId: string | undefined) => {
        if (!senderId) return 'Unknown';
        if (senderId === user?.id) return 'Me';
        const friend = friends.find(f => f.id === senderId);
        return friend ? friend.username : 'Unknown';
    };

    if (!isAuthenticated) {
        return <AuthScreen />;
    }

    const activeFriend = friends.find((f) => f.id === activeDM);

    return (
        <div className="flex h-screen w-full bg-background text-white overflow-hidden font-sans">
            {/* Left Sidebar - Friends List */}
            <div className="w-72 bg-surface flex flex-col border-r border-white/5 relative z-10 glass-panel">
                <div className="p-4 border-b border-white/5 flex items-center justify-between">
                    <h1 className="text-xl font-bold bg-gradient-to-r from-primary to-secondary bg-clip-text text-transparent">
                        P2P Nitro
                    </h1>
                    <div className="flex items-center gap-2">
                        {/* Connection indicator */}
                        <div
                            className={`p-1.5 rounded ${wsConnected ? 'text-green-400' : 'text-yellow-400'}`}
                            title={wsConnected ? 'Real-time connected' : 'Polling mode (5s)'}
                        >
                            {wsConnected ? <Wifi className="w-4 h-4" /> : <WifiOff className="w-4 h-4" />}
                        </div>
                        <button
                            onClick={() => setShowAddFriend(true)}
                            className="p-2 hover:bg-white/10 rounded-lg transition"
                            title="Add Friend"
                        >
                            <UserPlus className="w-5 h-5 text-gray-400" />
                        </button>
                    </div>
                </div>

                {/* Friends List */}
                <div className="flex-1 overflow-y-auto p-2 space-y-1">
                    {friends.map((friend) => {
                        const unreadCount = getUnreadCount(friend.id);
                        const isActive = activeDM === friend.id;
                        return (
                            <button
                                key={friend.id}
                                onClick={() => handleFriendClick(friend.id)}
                                className={`w-full flex items-center gap-3 p-3 rounded-lg transition relative ${isActive ? 'bg-primary/20 border border-primary/30' : 'hover:bg-white/5'
                                    }`}
                            >
                                {/* Avatar with online indicator */}
                                <div className="relative flex-shrink-0">
                                    <div className="w-12 h-12 rounded-full bg-gradient-to-br from-primary to-secondary" />
                                    <div className="absolute bottom-0 right-0 w-3.5 h-3.5 bg-green-500 rounded-full border-2 border-surface" />
                                </div>

                                {/* Username */}
                                <div className="flex-1 text-left">
                                    <div className="font-medium text-sm">{friend.username}</div>
                                    <div className="text-xs text-gray-400">Online</div>
                                </div>

                                {/* Unread badge */}
                                {unreadCount > 0 && (
                                    <div className="bg-red-500 text-white text-xs font-bold rounded-full min-w-[20px] h-5 flex items-center justify-center px-1.5">
                                        {unreadCount > 99 ? '99+' : unreadCount}
                                    </div>
                                )}
                            </button>
                        );
                    })}
                </div>

                {/* User Status */}
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
                                onClick={() => setIsMuted(!isMuted)}
                                className={`p-1.5 rounded ${isMuted ? 'bg-red-500/20 text-red-400' : 'hover:bg-white/10'}`}
                            >
                                {isMuted ? <MicOff className="w-4 h-4" /> : <Mic className="w-4 h-4" />}
                            </button>
                            <button onClick={logout} className="p-1.5 hover:bg-white/10 rounded text-gray-400">
                                <LogOut className="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            {/* Center - Chat Area */}
            <div className="flex-1 flex flex-col relative bg-background/50">
                {activeDM && activeFriend ? (
                    <>
                        {/* Chat Header */}
                        <div className="h-14 border-b border-white/5 flex items-center px-4 bg-surface/50 backdrop-blur-sm flex-shrink-0">
                            <div className="flex items-center text-gray-200">
                                <Hash className="w-5 h-5 text-gray-400 mr-2" />
                                <span className="font-semibold">{activeFriend.username}</span>
                            </div>
                        </div>

                        {/* Messages */}
                        <div className="flex-1 overflow-y-auto p-4 space-y-4">
                            {messages.length === 0 ? (
                                <div className="flex flex-col items-center justify-center h-full text-gray-500">
                                    <div className="w-24 h-24 rounded-full bg-gradient-to-br from-primary/20 to-secondary/20 flex items-center justify-center mb-4">
                                        <Hash className="w-12 h-12 text-primary/50" />
                                    </div>
                                    <p className="text-lg font-medium">Start the conversation!</p>
                                    <p className="text-sm">Send a message to {activeFriend.username}</p>
                                </div>
                            ) : (
                                <>
                                    {messages.map((msg) => {
                                        const displayContent = msg._decryptedContent || decryptMessageContent(msg);
                                        return (
                                            <div key={msg.id} className="flex group hover:bg-white/[0.02] -mx-4 px-4 py-1 transition">
                                                <div className="w-10 h-10 rounded-full bg-gray-700 mt-1 flex-shrink-0" />
                                                <div className="ml-3">
                                                    <div className="flex items-baseline">
                                                        <span className="font-medium text-gray-200">{getSenderName(msg.sender_id)}</span>
                                                        <span className="ml-2 text-xs text-gray-500">
                                                            {new Date(msg.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                                                        </span>
                                                        {msg.nonce && (
                                                            <span className="ml-2 text-xs text-green-500" title="End-to-end encrypted">
                                                                🔒
                                                            </span>
                                                        )}
                                                    </div>
                                                    <p className="text-gray-300">{displayContent}</p>
                                                </div>
                                            </div>
                                        );
                                    })}
                                    <div ref={messagesEndRef} />
                                </>
                            )}
                        </div>

                        {/* Input */}
                        <div className="p-4 pt-2 flex-shrink-0 border-t border-white/5">
                            <div className="relative bg-surface rounded-lg flex items-center p-1 ring-1 ring-white/10 focus-within:ring-primary/50 transition">
                                <input
                                    type="text"
                                    value={msgInput}
                                    onChange={(e) => setMsgInput(e.target.value)}
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter' && !e.shiftKey) {
                                            e.preventDefault();
                                            handleSendMessage();
                                        }
                                    }}
                                    placeholder={`Message ${activeFriend.username}`}
                                    className="bg-transparent flex-1 px-3 py-2 outline-none text-sm"
                                />
                                <button
                                    onClick={handleSendMessage}
                                    className="p-2 text-primary hover:text-primary/80 disabled:opacity-50 disabled:cursor-not-allowed"
                                    disabled={!msgInput.trim()}
                                >
                                    <Send className="w-5 h-5" />
                                </button>
                            </div>
                        </div>
                    </>
                ) : (
                    <div className="flex-1 flex items-center justify-center">
                        <div className="text-center text-gray-500">
                            <div className="w-32 h-32 rounded-full bg-gradient-to-br from-primary/20 to-secondary/20 flex items-center justify-center mx-auto mb-6">
                                <UserCircle className="w-16 h-16 text-primary/50" />
                            </div>
                            <p className="text-xl font-medium mb-2">Welcome back, {user?.username}!</p>
                            <p>Select a friend to start chatting</p>
                        </div>
                    </div>
                )}

                {/* Call Overlay */}
                {activeCall && (
                    <div className="absolute top-4 right-4 w-72 bg-black/80 backdrop-blur-xl border border-white/10 rounded-xl p-4 shadow-2xl z-20">
                        <div className="flex items-center justify-between mb-4">
                            <h3 className="text-sm font-bold text-white">Active Call</h3>
                            <button
                                onClick={endCall}
                                className="px-3 py-1 text-xs bg-red-600 hover:bg-red-500 rounded-lg transition"
                            >
                                End
                            </button>
                        </div>
                        <div className="space-y-3">
                            <div className="flex justify-between text-xs">
                                <span className="text-gray-500">Status</span>
                                <span className={activeCall.isConnected ? 'text-green-400' : 'text-yellow-400'}>
                                    {activeCall.isConnected ? 'Connected' : 'Connecting...'}
                                </span>
                            </div>
                        </div>
                    </div>
                )}
            </div>

            {/* Right Sidebar - Friend Info Panel */}
            {activeFriend && (
                <div className="w-80 bg-surface border-l border-white/5 flex flex-col animate-slide-in">
                    {/* Friend Profile Header */}
                    <div className="p-6 border-b border-white/5">
                        <div className="flex flex-col items-center text-center">
                            <div className="w-24 h-24 rounded-full bg-gradient-to-br from-primary to-secondary mb-4 relative">
                                <div className="absolute bottom-1 right-1 w-6 h-6 bg-green-500 rounded-full border-4 border-surface" />
                            </div>
                            <h2 className="text-xl font-bold mb-1">{activeFriend.username}</h2>
                            <p className="text-sm text-green-400 flex items-center gap-1">
                                <span className="w-2 h-2 bg-green-500 rounded-full" />
                                Online
                            </p>
                        </div>
                    </div>

                    {/* Action Buttons */}
                    <div className="p-4 border-b border-white/5 space-y-2">
                        <button
                            onClick={() => handleJoinCall(activeFriend.id)}
                            className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-green-600 hover:bg-green-500 text-white font-medium rounded-lg transition"
                        >
                            <Phone className="w-5 h-5" />
                            Voice Call
                        </button>
                        <button className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-blue-600 hover:bg-blue-500 text-white font-medium rounded-lg transition">
                            <Video className="w-5 h-5" />
                            Video Call
                        </button>
                    </div>

                    {/* Friend Info */}
                    <div className="flex-1 overflow-y-auto p-4 space-y-4">
                        {/* About Section */}
                        <div className="bg-black/20 rounded-lg p-4">
                            <h3 className="text-sm font-bold text-gray-400 uppercase mb-3">About</h3>
                            <div className="space-y-3">
                                <div className="flex items-center gap-3 text-sm">
                                    <Calendar className="w-4 h-4 text-gray-400" />
                                    <div>
                                        <div className="text-gray-400 text-xs">Member since</div>
                                        <div className="text-white">
                                            {activeFriend.last_seen ? new Date(activeFriend.last_seen).toLocaleDateString() : 'Recently'}
                                        </div>
                                    </div>
                                </div>
                                <div className="flex items-center gap-3 text-sm">
                                    <UserCircle className="w-4 h-4 text-gray-400" />
                                    <div>
                                        <div className="text-gray-400 text-xs">User ID</div>
                                        <div className="text-white text-xs font-mono">{activeFriend.id.slice(0, 8)}...</div>
                                    </div>
                                </div>
                            </div>
                        </div>

                        {/* E2EE Status */}
                        <div className="bg-black/20 rounded-lg p-4">
                            <h3 className="text-sm font-bold text-gray-400 uppercase mb-3">Security</h3>
                            <div className="flex items-center gap-2 text-green-400 text-sm">
                                <span>🔒</span>
                                <span>End-to-end encrypted</span>
                            </div>
                            <p className="text-xs text-gray-500 mt-2">
                                Messages are encrypted on your device. Only you and {activeFriend.username} can read them.
                            </p>
                        </div>
                    </div>

                    {/* Settings at bottom */}
                    <div className="p-4 border-t border-white/5">
                        <button className="w-full flex items-center justify-center gap-2 px-4 py-2 hover:bg-white/5 rounded-lg transition text-gray-400">
                            <Settings className="w-4 h-4" />
                            <span className="text-sm">More Options</span>
                        </button>
                    </div>
                </div>
            )}

            {/* Add Friend Modal */}
            <AddFriendModal isOpen={showAddFriend} onClose={() => setShowAddFriend(false)} />
        </div>
    );
}

export default App;
