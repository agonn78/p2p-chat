import { useState, useEffect } from 'react';
import { Mic, Video, Settings, Hash, Send, UserPlus, Phone, LogOut, MicOff } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from './store';
import { AuthScreen } from './components/AuthScreen';
import { FriendsList, AddFriendModal } from './components/FriendsList';

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

    const [activeView, setActiveView] = useState<'friends' | 'dm'>('friends');
    const [activeDM, setActiveDM] = useState<string | null>(null);
    const [msgInput, setMsgInput] = useState('');
    const [showAddFriend, setShowAddFriend] = useState(false);
    const [isMuted, setIsMuted] = useState(false);

    useEffect(() => {
        if (isAuthenticated) {
            fetchFriends();
        }
    }, [isAuthenticated, fetchFriends]);

    // WebSocket Listener
    useEffect(() => {
        if (!isAuthenticated) return;

        console.log("Setting up WS listener");
        const unlistenPromise = listen<string>('ws-message', (event) => {
            console.log("WS Event received:", event);
            try {
                const payload = JSON.parse(event.payload);
                if (payload.type === 'NEW_MESSAGE') {
                    console.log('[App] New message received:', payload.message);
                    addMessage(payload.message);
                }
            } catch (e) {
                console.error('Failed to parse WS message:', e);
            }
        });

        return () => {
            unlistenPromise.then((unlisten) => unlisten());
        };
    }, [isAuthenticated, addMessage]);

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
        setActiveView('dm');
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

    const getSenderName = (senderId: string) => {
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
            {/* Sidebar */}
            <div className="w-72 bg-surface flex flex-col border-r border-white/5 relative z-10 glass-panel">
                <div className="p-4 border-b border-white/5 flex items-center justify-between">
                    <h1 className="text-xl font-bold bg-gradient-to-r from-primary to-secondary bg-clip-text text-transparent">
                        P2P Nitro
                    </h1>
                    <button
                        onClick={() => setShowAddFriend(true)}
                        className="p-2 hover:bg-white/10 rounded-lg transition"
                        title="Add Friend"
                    >
                        <UserPlus className="w-5 h-5 text-gray-400" />
                    </button>
                </div>

                {/* Navigation */}
                <div className="flex border-b border-white/5">
                    <button
                        onClick={() => setActiveView('friends')}
                        className={`flex-1 py-3 text-sm font-medium transition ${activeView === 'friends'
                            ? 'text-primary border-b-2 border-primary'
                            : 'text-gray-400 hover:text-white'
                            }`}
                    >
                        Friends
                    </button>
                    <button
                        onClick={() => setActiveView('dm')}
                        className={`flex-1 py-3 text-sm font-medium transition ${activeView === 'dm'
                            ? 'text-primary border-b-2 border-primary'
                            : 'text-gray-400 hover:text-white'
                            }`}
                    >
                        Messages
                    </button>
                </div>

                {/* Content */}
                <div className="flex-1 overflow-hidden">
                    {activeView === 'friends' ? (
                        <FriendsList />
                    ) : (
                        <div className="p-2 space-y-1">
                            {friends.map((friend) => (
                                <button
                                    key={friend.id}
                                    onClick={() => handleFriendClick(friend.id)}
                                    className={`w-full flex items-center gap-3 p-2 rounded-lg transition ${activeDM === friend.id ? 'bg-primary/20' : 'hover:bg-white/5'
                                        }`}
                                >
                                    <div className="w-10 h-10 rounded-full bg-gradient-to-br from-primary to-secondary" />
                                    <span className="font-medium text-sm">{friend.username}</span>
                                </button>
                            ))}
                        </div>
                    )}
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

            {/* Main Content */}
            <div className="flex-1 flex flex-col relative bg-background/50">
                {activeDM && activeFriend ? (
                    <>
                        {/* Header */}
                        <div className="h-14 border-b border-white/5 flex items-center justify-between px-4 bg-surface/50 backdrop-blur-sm">
                            <div className="flex items-center text-gray-200">
                                <div className="w-8 h-8 rounded-full bg-gradient-to-br from-primary to-secondary mr-3" />
                                <span className="font-semibold">{activeFriend.username}</span>
                            </div>
                            <div className="flex items-center space-x-2">
                                <button
                                    onClick={() => handleJoinCall(activeFriend.id)}
                                    className="flex items-center px-4 py-2 bg-green-600 hover:bg-green-500 text-white text-sm rounded-lg transition shadow-lg shadow-green-900/20"
                                >
                                    <Phone className="w-4 h-4 mr-2" />
                                    Call
                                </button>
                                <button className="flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded-lg transition">
                                    <Video className="w-4 h-4 mr-2" />
                                    Video
                                </button>
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
                                messages.map((msg) => (
                                    <div key={msg.id} className="flex group hover:bg-white/[0.02] -mx-4 px-4 py-1 transition">
                                        <div className="w-10 h-10 rounded-full bg-gray-700 mt-1 flex-shrink-0" />
                                        <div className="ml-3">
                                            <div className="flex items-baseline">
                                                <span className="font-medium text-gray-200">{getSenderName(msg.sender_id)}</span>
                                                <span className="ml-2 text-xs text-gray-500">
                                                    {new Date(msg.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                                                </span>
                                            </div>
                                            <p className="text-gray-300">{msg.content}</p>
                                        </div>
                                    </div>
                                ))
                            )}
                        </div>

                        {/* Input */}
                        <div className="p-4 pt-2">
                            <div className="relative bg-surface rounded-lg flex items-center p-1 ring-1 ring-white/10 focus-within:ring-primary/50 transition">
                                <input
                                    type="text"
                                    value={msgInput}
                                    onChange={(e) => setMsgInput(e.target.value)}
                                    // onKeyDown={(e) => e.key === 'Enter' && handleSendMessage()}
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') {
                                            handleSendMessage();
                                        }
                                    }}
                                    placeholder={`Message ${activeFriend.username}`}
                                    className="bg-transparent flex-1 px-3 py-2 outline-none text-sm"
                                />
                                <button
                                    onClick={handleSendMessage}
                                    className="p-2 text-primary hover:text-primary/80">
                                    <Send className="w-5 h-5" />
                                </button>
                            </div>
                        </div>
                    </>
                ) : (
                    <div className="flex-1 flex items-center justify-center">
                        <div className="text-center text-gray-500">
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

            {/* Add Friend Modal */}
            <AddFriendModal isOpen={showAddFriend} onClose={() => setShowAddFriend(false)} />
        </div>
    );
}

export default App;
