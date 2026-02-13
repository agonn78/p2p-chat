import { useState, useEffect, useRef } from 'react';
import { Mic, Video, Settings, Hash, Send, UserPlus, Phone, LogOut, MicOff, UserCircle, Calendar, Wifi, WifiOff, Trash2, Copy, Pencil, Check, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore, POLL_INTERVAL_MS } from './store';
import { AuthScreen } from './components/AuthScreen';
import { AddFriendModal } from './components/FriendsList';
import { MessageContent } from './components/MessageContent';
import { ServerSidebar } from './components/ServerSidebar';
import { ChannelList } from './components/ChannelList';
import { MemberList } from './components/MemberList';
import { IncomingCallModal } from './components/IncomingCallModal';
import { SafeCallOverlay } from './components/SafeCallOverlay';
import type { IncomingCallPayload, CallAcceptedPayload } from './types';

// Slash commands
const SLASH_COMMANDS: Record<string, { description: string; replacement?: string }> = {
    '/shrug': { description: 'Sends ¯\\_(ツ)_/¯', replacement: '¯\\_(ツ)_/¯' },
    '/tableflip': { description: 'Sends (╯°□°)╯︵ ┻━┻', replacement: '(╯°□°)╯︵ ┻━┻' },
    '/unflip': { description: 'Sends ┬─┬ノ( º _ ºノ)', replacement: '┬─┬ノ( º _ ºノ)' },
    '/lenny': { description: 'Sends ( ͡° ͜ʖ ͡°)', replacement: '( ͡° ͜ʖ ͡°)' },
    '/disapprove': { description: 'Sends ಠ_ಠ', replacement: 'ಠ_ಠ' },
    '/clear': { description: 'Clear local chat view' },
    '/deleteall': { description: 'Delete entire conversation (server)' },
};

function App() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const user = useAppStore((s) => s.user);
    const logout = useAppStore((s) => s.logout);
    const activeCall = useAppStore((s) => s.activeCall);
    const endCall = useAppStore((s) => s.endCall);
    const friends = useAppStore((s) => s.friends);
    const startCall = useAppStore((s) => s.startCall);
    const fetchFriends = useAppStore((s) => s.fetchFriends);

    // Server Store
    const fetchServers = useAppStore((s) => s.fetchServers);
    const activeServer = useAppStore((s) => s.activeServer);
    const channels = useAppStore((s) => s.channels);
    const activeChannel = useAppStore((s) => s.activeChannel);
    const channelMessages = useAppStore((s) => s.channelMessages);
    const hasMoreChannelMessages = useAppStore((s) => s.hasMoreChannelMessages);
    const isLoadingMoreChannelMessages = useAppStore((s) => s.isLoadingMoreChannelMessages);
    const loadOlderChannelMessages = useAppStore((s) => s.loadOlderChannelMessages);
    const serverMembers = useAppStore((s) => s.serverMembers);
    const typingByChannel = useAppStore((s) => s.typingByChannel);
    const setChannelTyping = useAppStore((s) => s.setChannelTyping);
    const sendChannelMessage = useAppStore((s) => s.sendChannelMessage);

    // Chat Store
    const activeRoom = useAppStore((s) => s.activeRoom);
    const messages = useAppStore((s) => s.messages);
    const hasMoreMessages = useAppStore((s) => s.hasMoreMessages);
    const isLoadingMoreMessages = useAppStore((s) => s.isLoadingMoreMessages);
    const loadOlderMessages = useAppStore((s) => s.loadOlderMessages);
    const typingByRoom = useAppStore((s) => s.typingByRoom);
    const setTyping = useAppStore((s) => s.setTyping);
    const createOrGetDm = useAppStore((s) => s.createOrGetDm);
    const sendMessage = useAppStore((s) => s.sendMessage);
    const addMessage = useAppStore((s) => s.addMessage);
    const removeMessage = useAppStore((s) => s.removeMessage);
    const updateMessage = useAppStore((s) => s.updateMessage);
    const deleteMessage = useAppStore((s) => s.deleteMessage);
    const editMessage = useAppStore((s) => s.editMessage);
    const deleteAllMessages = useAppStore((s) => s.deleteAllMessages);
    const clearMessages = useAppStore((s) => s.clearMessages);
    const getUnreadCount = useAppStore((s) => s.getUnreadCount);
    const decryptMessageContent = useAppStore((s) => s.decryptMessageContent);
    const pollForNewMessages = useAppStore((s) => s.pollForNewMessages);

    // Call Store
    const handleIncomingCall = useAppStore((s) => s.handleIncomingCall);

    // Connection state
    const wsConnected = useAppStore((s) => s.wsConnected);
    const setWsConnected = useAppStore((s) => s.setWsConnected);
    const onlineFriends = useAppStore((s) => s.onlineFriends);

    const [activeDM, setActiveDM] = useState<string | null>(null);
    const [msgInput, setMsgInput] = useState('');
    const [showAddFriend, setShowAddFriend] = useState(false);
    const [isMuted, setIsMuted] = useState(false);
    const [hoveredMessageId, setHoveredMessageId] = useState<string | null>(null);
    const [showCommandHint, setShowCommandHint] = useState(false);
    const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
    const [editInput, setEditInput] = useState('');

    // Refs for chat containers
    const dmMessagesContainerRef = useRef<HTMLDivElement>(null);
    const messagesEndRef = useRef<HTMLDivElement>(null);

    const typingTimeoutRef = useRef<number | null>(null);
    const dmLoadOlderInFlightRef = useRef(false);

    useEffect(() => {
        if (isAuthenticated) {
            fetchFriends();
            fetchServers();
            invoke<number>('api_drain_outbox', { limit: 200 }).catch(() => undefined);
        }
    }, [isAuthenticated, fetchFriends, fetchServers]);

    // WebSocket Listener with connection tracking
    useEffect(() => {
        if (!isAuthenticated) {
            console.log('[App] Not authenticated, skipping WS listener setup');
            return;
        }

        console.log('[App] 🔌 Setting up WebSocket listener...');

        const setupListener = async () => {
            try {
                const unlisten = await listen<string>('ws-message', (event) => {
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

                            if (payload.message?.room_id && payload.message?.id && payload.message?.sender_id !== user?.id) {
                                invoke('api_mark_message_delivered', {
                                    roomId: payload.message.room_id,
                                    messageId: payload.message.id,
                                }).catch(() => undefined);

                                if (payload.message.room_id === activeRoom) {
                                    invoke('api_mark_room_read', {
                                        roomId: payload.message.room_id,
                                        uptoMessageId: payload.message.id,
                                    }).catch(() => undefined);
                                }
                            }
                        } else if (payload.type === 'MESSAGE_EDITED') {
                            console.log('[App] ✏️ MESSAGE_EDITED via WebSocket');
                            updateMessage(payload.message);
                        } else if (payload.type === 'MESSAGE_STATUS') {
                            useAppStore.setState((state) => ({
                                messages: state.messages.map((m) =>
                                    m.id === payload.message_id ? { ...m, status: payload.status } : m
                                ),
                            }));
                        } else if (payload.type === 'TYPING') {
                            if (payload.room_id && payload.user_id) {
                                setTyping(payload.room_id, payload.user_id, !!payload.is_typing);
                            }
                        } else if (payload.type === 'MESSAGE_DELETED') {
                            console.log('[App] 🗑️ MESSAGE_DELETED via WebSocket');
                            removeMessage(payload.message_id);
                        } else if (payload.type === 'ALL_MESSAGES_DELETED') {
                            console.log('[App] 🗑️ ALL_MESSAGES_DELETED via WebSocket');
                            if (payload.room_id === activeRoom) {
                                clearMessages();
                            }
                        } else if (payload.type === 'NEW_CHANNEL_MESSAGE') {
                            if (payload.channel_id === activeChannel) {
                                useAppStore.setState((state) => {
                                    if (state.channelMessages.some((m) =>
                                        m.id === payload.message.id ||
                                        (!!m.client_id && !!payload.message.client_id && m.client_id === payload.message.client_id)
                                    )) {
                                        return state;
                                    }
                                    return {
                                        channelMessages: [...state.channelMessages, payload.message],
                                    };
                                });
                            }
                        } else if (payload.type === 'CHANNEL_TYPING') {
                            if (payload.channel_id && payload.user_id) {
                                setChannelTyping(payload.channel_id, payload.user_id, !!payload.is_typing);
                            }
                        } else if (payload.type === 'CHANNEL_MESSAGE_EDITED') {
                            useAppStore.setState((state) => ({
                                channelMessages: state.channelMessages.map((m) =>
                                    m.id === payload.message.id ? { ...m, ...payload.message } : m
                                ),
                            }));
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

                const unlistenReconnected = await listen<boolean>('ws-reconnected', async () => {
                    if (user?.id) {
                        try {
                            await invoke('identify_user', { userId: user.id });
                            console.log('[App] 🔁 Re-identified user after WS reconnect');
                        } catch (err) {
                            console.error('[App] Failed to re-identify after reconnect:', err);
                        }
                    }

                    try {
                        const retried = await invoke<number>('api_drain_outbox', { limit: 200 });
                        if (retried > 0) {
                            console.log(`[App] 🔁 Drained ${retried} queued messages after reconnect`);
                        }
                    } catch (err) {
                        console.warn('[App] Failed to drain outbox after reconnect:', err);
                    }
                });

                console.log('[App] ✅ WebSocket listeners attached');
                setWsConnected(true);

                return () => {
                    unlisten();
                    unlistenStatus();
                    unlistenReconnected();
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
    }, [isAuthenticated, activeRoom, activeChannel, user?.id, setTyping, setChannelTyping]);

    // Call event listeners
    useEffect(() => {
        if (!isAuthenticated) return;

        const setupCallListeners = async () => {
            const unlistenIncoming = await listen<IncomingCallPayload>('incoming-call', (event) => {
                console.log('[App] 📞 Incoming call event:', event.payload);
                handleIncomingCall(event.payload);
            });

            const unlistenAccepted = await listen<CallAcceptedPayload>('call-accepted', async (event) => {
                console.log('[CALL-DEBUG] ===== CALL ACCEPTED EVENT =====');
                console.log('[CALL-DEBUG] Payload:', JSON.stringify(event.payload, null, 2));

                // Complete E2EE handshake on caller side
                try {
                    await invoke('complete_call_handshake', { peerPublicKey: event.payload.publicKey });
                    console.log('[CALL-DEBUG] ✅ complete_call_handshake succeeded');

                    // Start WebRTC Audio Handshake
                    console.log('[CALL-DEBUG] Initializing WebRTC audio call...');
                    try {
                        await invoke('init_audio_call', { targetId: event.payload.peerId });
                        console.log('[CALL-DEBUG] ✅ WebRTC audio call initialized');
                    } catch (audioErr) {
                        console.warn('[CALL-DEBUG] ⚠️ Audio start failed:', audioErr);
                    }

                    useAppStore.setState((state) => ({
                        activeCall: state.activeCall ? {
                            ...state.activeCall,
                            status: 'connected',
                            startTime: Date.now(),
                        } : null,
                    }));
                } catch (e) {
                    console.error('[CALL-DEBUG] ❌ E2EE handshake failed:', e);
                    endCall();
                }
            });

            // WebRTC Listeners
            const unlistenOffer = await listen<any>('webrtc-offer', async (event) => {
                console.log('[WEBRTC] Received Offer, handling...');
                try {
                    await invoke('handle_audio_offer', {
                        targetId: event.payload.peerId,
                        sdp: event.payload.sdp
                    });
                    // Note: Callee starts sending media after handling offer (creating answer)
                } catch (e) {
                    console.error('[WEBRTC] Failed to handle offer:', e);
                }
            });

            const unlistenAnswer = await listen<any>('webrtc-answer', async (event) => {
                console.log('[WEBRTC] Received Answer, handling...');
                try {
                    await invoke('handle_audio_answer', { sdp: event.payload.sdp });
                } catch (e) {
                    console.error('[WEBRTC] Failed to handle answer:', e);
                }
            });

            const unlistenCandidate = await listen<any>('webrtc-candidate', async (event) => {
                // console.log('[WEBRTC] Received ICE candidate');
                try {
                    await invoke('handle_ice_candidate', { payload: event.payload });
                } catch (e) {
                    console.error('[WEBRTC] Failed to handle candidate:', e);
                }
            });

            const unlistenDeclined = await listen<string>('call-declined', (event) => {
                console.log('[CALL-DEBUG] ===== CALL DECLINED EVENT =====');
                useAppStore.setState({ activeCall: null });
            });

            const unlistenEnded = await listen<string>('call-ended', (event) => {
                console.log('[CALL-DEBUG] ===== CALL ENDED EVENT =====');
                useAppStore.setState({ activeCall: null });
            });

            const unlistenBusy = await listen<string>('call-busy', () => {
                console.log('[App] 📳 Target is busy');
                useAppStore.setState({ activeCall: null });
            });

            const unlistenCancelled = await listen<string>('call-cancelled', () => {
                console.log('[App] 🚫 Call cancelled by caller');
                useAppStore.setState({ activeCall: null });
            });

            return () => {
                unlistenIncoming();
                unlistenAccepted();
                unlistenOffer();
                unlistenAnswer();
                unlistenCandidate();
                unlistenDeclined();
                unlistenEnded();
                unlistenBusy();
                unlistenCancelled();
            };
        };

        let cleanup: (() => void) | null = null;
        setupCallListeners().then(fn => { cleanup = fn; });

        return () => {
            if (cleanup) cleanup();
        };
    }, [isAuthenticated, handleIncomingCall, endCall]);

    // Polling fallback
    useEffect(() => {
        if (!isAuthenticated || !activeRoom) {
            return;
        }

        console.log(`[App] 📊 Starting polling (interval: ${POLL_INTERVAL_MS}ms)`);

        const pollInterval = setInterval(() => {
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

    // Mark active DM as read (receipts)
    useEffect(() => {
        if (!activeRoom || messages.length === 0) {
            return;
        }

        const latest = messages[messages.length - 1];
        if (!latest?.id || latest.sender_id === user?.id) {
            return;
        }

        invoke('api_mark_room_read', { roomId: activeRoom, uptoMessageId: latest.id }).catch(() => undefined);
    }, [activeRoom, messages.length, user?.id]);

    // Auto-scroll DM only if user is already near bottom
    useEffect(() => {
        const container = dmMessagesContainerRef.current;
        if (!container) return;

        const distanceToBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
        if (distanceToBottom < 120) {
            messagesEndRef.current?.scrollIntoView({ behavior: 'auto' });
        }
    }, [messages.length, activeRoom]);

    // Command hint visibility
    useEffect(() => {
        setShowCommandHint(msgInput.startsWith('/') && msgInput.length > 1);
    }, [msgInput]);

    // Typing indicator emitter (DM only)
    useEffect(() => {
        if (!isAuthenticated) return;

        const isTyping = msgInput.trim().length > 0;

        if (activeRoom) {
            invoke('api_send_typing', { roomId: activeRoom, isTyping }).catch(() => undefined);
            if (typingTimeoutRef.current) {
                window.clearTimeout(typingTimeoutRef.current);
            }
            if (isTyping) {
                typingTimeoutRef.current = window.setTimeout(() => {
                    invoke('api_send_typing', { roomId: activeRoom, isTyping: false }).catch(() => undefined);
                }, 2500);
            }
        }

        return () => {
            if (typingTimeoutRef.current) {
                window.clearTimeout(typingTimeoutRef.current);
                typingTimeoutRef.current = null;
            }
        };
    }, [msgInput, activeRoom, isAuthenticated]);

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

        const input = msgInput.trim();

        // Handle slash commands
        if (input.startsWith('/')) {
            const command = input.split(' ')[0].toLowerCase();

            if (command === '/clear') {
                clearMessages();
                setMsgInput('');
                return;
            }

            if (command === '/deleteall') {
                if (confirm('Delete ALL messages in this conversation? This cannot be undone.')) {
                    await deleteAllMessages();
                }
                setMsgInput('');
                return;
            }

            // Text replacement commands
            const cmd = SLASH_COMMANDS[command];
            if (cmd?.replacement) {
                await sendMessage(activeRoom, cmd.replacement);
                setMsgInput('');
                return;
            }
        }

        // Normal message
        try {
            await sendMessage(activeRoom, input);
            setMsgInput('');
        } catch (e) {
            console.error('Failed to send message:', e);
        }
    };

    const handleDeleteMessage = async (messageId: string) => {
        if (confirm('Delete this message?')) {
            await deleteMessage(messageId);
        }
    };

    const handleCopyMessage = (content: string) => {
        navigator.clipboard.writeText(content);
    };

    const startEditingMessage = (messageId: string, content: string) => {
        setEditingMessageId(messageId);
        setEditInput(content);
    };

    const cancelEditingMessage = () => {
        setEditingMessageId(null);
        setEditInput('');
    };

    const saveEditedMessage = async () => {
        if (!editingMessageId || !editInput.trim()) return;
        await editMessage(editingMessageId, editInput.trim());
        cancelEditingMessage();
    };

    const getSenderName = (senderId: string | undefined) => {
        if (!senderId) return 'Unknown';
        if (senderId === user?.id) return 'Me';
        const friend = friends.find(f => f.id === senderId);
        if (friend) return friend.username;
        const member = serverMembers.find((m) => m.user_id === senderId);
        return member ? member.username : 'Unknown';
    };

    // Get matching commands for hint
    const getMatchingCommands = () => {
        if (!msgInput.startsWith('/')) return [];
        return Object.entries(SLASH_COMMANDS)
            .filter(([cmd]) => cmd.startsWith(msgInput.toLowerCase()))
            .slice(0, 5);
    };

    if (!isAuthenticated) {
        return <AuthScreen />;
    }

    const activeFriend = friends.find((f) => f.id === activeDM);
    const callOverlayResetKey = activeCall ? `${activeCall.status}:${activeCall.peerId ?? 'none'}` : 'none';

    return (
        <div className="flex h-screen w-full bg-background text-white overflow-hidden font-sans">
            {/* Call UI Components */}
            <IncomingCallModal />
            <SafeCallOverlay resetKey={callOverlayResetKey} />

            {/* Far Left - Server Sidebar */}
            <ServerSidebar />

            {/* Conditional: Server View or DM View */}
            {activeServer ? (
                <>
                    {/* Channel List */}
                    <ChannelList />

                    {/* Server Chat Area */}
                    <div className="flex-1 flex flex-col relative bg-background/50">
                        {activeChannel ? (
                            <ServerChatView
                                channelMessages={channelMessages}
                                sendChannelMessage={sendChannelMessage}
                                loadOlderChannelMessages={loadOlderChannelMessages}
                                hasMoreChannelMessages={hasMoreChannelMessages}
                                isLoadingMoreChannelMessages={isLoadingMoreChannelMessages}
                                activeServer={activeServer}
                                activeChannel={activeChannel}
                                channels={channels}
                                user={user}
                                serverMembers={serverMembers}
                                typingUserIds={typingByChannel[activeChannel] || []}
                            />
                        ) : (
                            <div className="flex-1 flex items-center justify-center">
                                <div className="text-center text-gray-500">
                                    <Hash className="w-16 h-16 mx-auto mb-4 opacity-50" />
                                    <p className="text-lg font-medium">Select a channel</p>
                                    <p className="text-sm">Choose a channel to start chatting</p>
                                </div>
                            </div>
                        )}
                    </div>

                    {/* Member List */}
                    <MemberList />
                </>
            ) : (
                <>
                    {/* Left Sidebar - Friends List */}
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
                                const isOnline = onlineFriends.includes(friend.id);
                                return (
                                    <button
                                        key={friend.id}
                                        onClick={() => handleFriendClick(friend.id)}
                                        className={`w-full flex items-center gap-3 p-3 rounded-lg transition relative ${isActive ? 'bg-primary/20 border border-primary/30' : 'hover:bg-white/5'
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
                                <div
                                    ref={dmMessagesContainerRef}
                                    className="flex-1 overflow-y-auto p-4 space-y-2"
                                    onScroll={(e) => {
                                        const el = e.currentTarget;
                                        if (
                                            el.scrollTop < 80
                                            && hasMoreMessages
                                            && !isLoadingMoreMessages
                                            && !dmLoadOlderInFlightRef.current
                                        ) {
                                            const previousHeight = el.scrollHeight;
                                            const previousTop = el.scrollTop;
                                            dmLoadOlderInFlightRef.current = true;

                                            void loadOlderMessages().finally(() => {
                                                window.requestAnimationFrame(() => {
                                                    const updated = dmMessagesContainerRef.current;
                                                    if (updated) {
                                                        const delta = updated.scrollHeight - previousHeight;
                                                        updated.scrollTop = previousTop + Math.max(0, delta);
                                                    }
                                                    dmLoadOlderInFlightRef.current = false;
                                                });
                                            });
                                        }
                                    }}
                                >
                                    {(hasMoreMessages || isLoadingMoreMessages) && (
                                        <div className="flex justify-center mb-3">
                                            <button
                                                onClick={() => {
                                                    const container = dmMessagesContainerRef.current;
                                                    if (
                                                        !container
                                                        || isLoadingMoreMessages
                                                        || !hasMoreMessages
                                                        || dmLoadOlderInFlightRef.current
                                                    ) {
                                                        return;
                                                    }

                                                    const previousHeight = container.scrollHeight;
                                                    const previousTop = container.scrollTop;
                                                    dmLoadOlderInFlightRef.current = true;

                                                    void loadOlderMessages().finally(() => {
                                                        window.requestAnimationFrame(() => {
                                                            const updated = dmMessagesContainerRef.current;
                                                            if (updated) {
                                                                const delta = updated.scrollHeight - previousHeight;
                                                                updated.scrollTop = previousTop + Math.max(0, delta);
                                                            }
                                                            dmLoadOlderInFlightRef.current = false;
                                                        });
                                                    });
                                                }}
                                                disabled={isLoadingMoreMessages}
                                                className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10 disabled:opacity-60"
                                            >
                                                {isLoadingMoreMessages ? 'Loading older messages...' : 'Load older messages'}
                                            </button>
                                        </div>
                                    )}
                                    {messages.length === 0 ? (
                                        <div className="flex flex-col items-center justify-center h-full text-gray-500">
                                            <div className="w-24 h-24 rounded-full bg-gradient-to-br from-primary/20 to-secondary/20 flex items-center justify-center mb-4">
                                                <Hash className="w-12 h-12 text-primary/50" />
                                            </div>
                                            <p className="text-lg font-medium">Start the conversation!</p>
                                            <p className="text-sm">Send a message to {activeFriend.username}</p>
                                            <p className="text-xs text-gray-600 mt-4">💡 Try /shrug, /tableflip, or **bold text**</p>
                                        </div>
                                    ) : (
                                        <>
                                            {messages.map((msg) => {
                                                const displayContent = msg._decryptedContent || decryptMessageContent(msg);
                                                const isOwn = msg.sender_id === user?.id;
                                                const isHovered = hoveredMessageId === msg.id;
                                                const isEditing = editingMessageId === msg.id;

                                                return (
                                                    <div
                                                        key={msg.id}
                                                        className="flex group hover:bg-white/[0.02] -mx-4 px-4 py-1.5 transition relative"
                                                        onMouseEnter={() => setHoveredMessageId(msg.id)}
                                                        onMouseLeave={() => setHoveredMessageId(null)}
                                                    >
                                                        <div className="w-10 h-10 rounded-full bg-gray-700 mt-1 flex-shrink-0" />
                                                        <div className="ml-3 flex-1 min-w-0">
                                                            <div className="flex items-baseline gap-2">
                                                                <span className={`font-medium ${isOwn ? 'text-primary' : 'text-gray-200'}`}>
                                                                    {getSenderName(msg.sender_id)}
                                                                </span>
                                                                <span className="text-xs text-gray-500">
                                                                    {new Date(msg.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                                                                </span>
                                                                {isOwn && msg.status && (
                                                                    <span className="text-xs text-gray-500 capitalize">{msg.status}</span>
                                                                )}
                                                                {msg.nonce && (
                                                                    <span className="text-xs text-green-500" title="End-to-end encrypted">
                                                                        🔒
                                                                    </span>
                                                                )}
                                                            </div>
                                                            {isEditing ? (
                                                                <div className="mt-1 flex items-center gap-2">
                                                                    <input
                                                                        value={editInput}
                                                                        onChange={(e) => setEditInput(e.target.value)}
                                                                        className="flex-1 bg-black/30 border border-white/10 rounded px-2 py-1 text-sm outline-none focus:border-primary/50"
                                                                        onKeyDown={(e) => {
                                                                            if (e.key === 'Enter') saveEditedMessage();
                                                                            if (e.key === 'Escape') cancelEditingMessage();
                                                                        }}
                                                                        autoFocus
                                                                    />
                                                                    <button
                                                                        onClick={saveEditedMessage}
                                                                        className="p-1.5 hover:bg-green-500/20 rounded text-green-400 transition"
                                                                        title="Save edit"
                                                                    >
                                                                        <Check className="w-4 h-4" />
                                                                    </button>
                                                                    <button
                                                                        onClick={cancelEditingMessage}
                                                                        className="p-1.5 hover:bg-red-500/20 rounded text-red-400 transition"
                                                                        title="Cancel edit"
                                                                    >
                                                                        <X className="w-4 h-4" />
                                                                    </button>
                                                                </div>
                                                            ) : (
                                                                <div className="text-gray-300">
                                                                    <MessageContent content={displayContent} isEncrypted={!!msg.nonce} />
                                                                </div>
                                                            )}
                                                        </div>

                                                        {/* Action buttons on hover */}
                                                        {isHovered && (
                                                            <div className="absolute right-4 top-1/2 -translate-y-1/2 flex items-center gap-1 bg-surface/90 backdrop-blur-sm rounded-lg p-1 border border-white/10">
                                                                <button
                                                                    onClick={() => handleCopyMessage(displayContent)}
                                                                    className="p-1.5 hover:bg-white/10 rounded text-gray-400 hover:text-white transition"
                                                                    title="Copy message"
                                                                >
                                                                    <Copy className="w-4 h-4" />
                                                                </button>
                                                                {isOwn && (
                                                                    <button
                                                                        onClick={() => startEditingMessage(msg.id, displayContent)}
                                                                        className="p-1.5 hover:bg-white/10 rounded text-gray-400 hover:text-white transition"
                                                                        title="Edit message"
                                                                    >
                                                                        <Pencil className="w-4 h-4" />
                                                                    </button>
                                                                )}
                                                                {isOwn && (
                                                                    <button
                                                                        onClick={() => handleDeleteMessage(msg.id)}
                                                                        className="p-1.5 hover:bg-red-500/20 rounded text-gray-400 hover:text-red-400 transition"
                                                                        title="Delete message"
                                                                    >
                                                                        <Trash2 className="w-4 h-4" />
                                                                    </button>
                                                                )}
                                                            </div>
                                                        )}
                                                    </div>
                                                );
                                            })}
                                            <div ref={messagesEndRef} />
                                        </>
                                    )}
                                </div>

                                {activeRoom && (typingByRoom[activeRoom] || []).length > 0 && (
                                    <div className="px-4 pb-1 text-xs text-gray-400">
                                        {(typingByRoom[activeRoom] || [])
                                            .filter((id) => id !== user?.id)
                                            .map((id) => getSenderName(id))[0] || 'Someone'} is typing...
                                    </div>
                                )}

                                {/* Command hints */}
                                {showCommandHint && getMatchingCommands().length > 0 && (
                                    <div className="mx-4 mb-2 bg-surface rounded-lg border border-white/10 overflow-hidden">
                                        {getMatchingCommands().map(([cmd, info]) => (
                                            <button
                                                key={cmd}
                                                onClick={() => setMsgInput(cmd)}
                                                className="w-full flex items-center justify-between px-4 py-2 hover:bg-white/5 transition text-left"
                                            >
                                                <span className="text-primary font-mono">{cmd}</span>
                                                <span className="text-gray-500 text-sm">{info.description}</span>
                                            </button>
                                        ))}
                                    </div>
                                )}

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
                                            placeholder={`Message ${activeFriend.username} • Markdown supported`}
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

                    </div>

                    {/* Right Sidebar - Friend Info Panel */}
                    {activeFriend && (
                        <div className="w-80 bg-surface border-l border-white/5 flex flex-col animate-slide-in">
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

                            <div className="flex-1 overflow-y-auto p-4 space-y-4">
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
                    )}

                    <AddFriendModal isOpen={showAddFriend} onClose={() => setShowAddFriend(false)} />
                </>
            )}
        </div>
    );
}

// Server Chat View Component
function ServerChatView({
    channelMessages,
    sendChannelMessage,
    loadOlderChannelMessages,
    hasMoreChannelMessages,
    isLoadingMoreChannelMessages,
    activeServer,
    activeChannel,
    channels,
    user,
    serverMembers,
    typingUserIds,
}: {
    channelMessages: any[];
    sendChannelMessage: (serverId: string, channelId: string, content: string) => Promise<void>;
    loadOlderChannelMessages: () => Promise<void>;
    hasMoreChannelMessages: boolean;
    isLoadingMoreChannelMessages: boolean;
    activeServer: string;
    activeChannel: string;
    channels: any[];
    user: any;
    serverMembers: any[];
    typingUserIds: string[];
}) {
    const [input, setInput] = useState('');
    const messagesContainerRef = useRef<HTMLDivElement>(null);
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const typingTimeoutRef = useRef<number | null>(null);
    const channelLoadOlderInFlightRef = useRef(false);
    const channel = channels.find((c: any) => c.id === activeChannel);
    const typingMemberName = typingUserIds
        .filter((id) => id !== user?.id)
        .map((id) => serverMembers.find((m: any) => m.user_id === id)?.username || 'Someone')[0];

    useEffect(() => {
        const isTyping = input.trim().length > 0;

        invoke('api_send_channel_typing', {
            serverId: activeServer,
            channelId: activeChannel,
            isTyping,
        }).catch(() => undefined);

        if (typingTimeoutRef.current) {
            window.clearTimeout(typingTimeoutRef.current);
        }

        if (isTyping) {
            typingTimeoutRef.current = window.setTimeout(() => {
                invoke('api_send_channel_typing', {
                    serverId: activeServer,
                    channelId: activeChannel,
                    isTyping: false,
                }).catch(() => undefined);
            }, 2500);
        }

        return () => {
            if (typingTimeoutRef.current) {
                window.clearTimeout(typingTimeoutRef.current);
                typingTimeoutRef.current = null;
            }
        };
    }, [input, activeServer, activeChannel]);

    useEffect(() => {
        const container = messagesContainerRef.current;
        if (!container) return;

        const distanceToBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
        if (distanceToBottom < 120) {
            messagesEndRef.current?.scrollIntoView({ behavior: 'auto' });
        }
    }, [channelMessages.length]);

    const handleSend = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!input.trim()) return;
        await sendChannelMessage(activeServer, activeChannel, input);
        setInput('');
    };

    return (
        <>
            {/* Channel Header */}
            <div className="h-14 px-4 flex items-center border-b border-white/5">
                <Hash className="w-5 h-5 text-gray-400 mr-2" />
                <h2 className="font-bold">{channel?.name || 'Channel'}</h2>
            </div>

            {/* Messages */}
            <div
                ref={messagesContainerRef}
                className="flex-1 overflow-y-auto p-4 space-y-4"
                onScroll={(e) => {
                    const el = e.currentTarget;
                    if (
                        el.scrollTop < 80
                        && hasMoreChannelMessages
                        && !isLoadingMoreChannelMessages
                        && !channelLoadOlderInFlightRef.current
                    ) {
                        const previousHeight = el.scrollHeight;
                        const previousTop = el.scrollTop;
                        channelLoadOlderInFlightRef.current = true;

                        void loadOlderChannelMessages().finally(() => {
                            window.requestAnimationFrame(() => {
                                const updated = messagesContainerRef.current;
                                if (updated) {
                                    const delta = updated.scrollHeight - previousHeight;
                                    updated.scrollTop = previousTop + Math.max(0, delta);
                                }
                                channelLoadOlderInFlightRef.current = false;
                            });
                        });
                    }
                }}
            >
                {(hasMoreChannelMessages || isLoadingMoreChannelMessages) && (
                    <div className="flex justify-center mb-2">
                        <button
                            onClick={() => {
                                const container = messagesContainerRef.current;
                                if (
                                    !container
                                    || isLoadingMoreChannelMessages
                                    || !hasMoreChannelMessages
                                    || channelLoadOlderInFlightRef.current
                                ) {
                                    return;
                                }

                                const previousHeight = container.scrollHeight;
                                const previousTop = container.scrollTop;
                                channelLoadOlderInFlightRef.current = true;

                                void loadOlderChannelMessages().finally(() => {
                                    window.requestAnimationFrame(() => {
                                        const updated = messagesContainerRef.current;
                                        if (updated) {
                                            const delta = updated.scrollHeight - previousHeight;
                                            updated.scrollTop = previousTop + Math.max(0, delta);
                                        }
                                        channelLoadOlderInFlightRef.current = false;
                                    });
                                });
                            }}
                            disabled={isLoadingMoreChannelMessages}
                            className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10 disabled:opacity-60"
                        >
                            {isLoadingMoreChannelMessages ? 'Loading older messages...' : 'Load older messages'}
                        </button>
                    </div>
                )}
                {channelMessages.map((msg) => (
                    <div key={msg.id} className="flex gap-3 hover:bg-white/5 p-2 rounded-lg">
                        <div className="w-10 h-10 rounded-full bg-gradient-to-br from-primary to-secondary flex-shrink-0" />
                        <div>
                            <div className="flex items-center gap-2">
                                <span className="font-semibold text-sm">
                                    {msg.sender_id === user?.id
                                        ? user?.username
                                        : msg.sender_username || serverMembers.find((m: any) => m.user_id === msg.sender_id)?.username || 'Member'}
                                </span>
                                <span className="text-xs text-gray-500">
                                    {new Date(msg.created_at).toLocaleTimeString()}
                                </span>
                                {msg.status && msg.sender_id === user?.id && (
                                    <span className="text-xs text-gray-500 capitalize">{msg.status}</span>
                                )}
                            </div>
                            <p className="text-gray-200">{msg.content}</p>
                        </div>
                    </div>
                ))}
                <div ref={messagesEndRef} />
            </div>

            {typingMemberName && (
                <div className="px-4 pb-1 text-xs text-gray-400">{typingMemberName} is typing...</div>
            )}

            {/* Input */}
            <form onSubmit={handleSend} className="p-4 border-t border-white/5">
                <div className="flex items-center gap-2 bg-white/5 rounded-lg px-4 py-2">
                    <input
                        type="text"
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        placeholder={`Message #${channel?.name || 'channel'}`}
                        className="flex-1 bg-transparent outline-none"
                    />
                    <button type="submit" className="p-2 hover:bg-white/10 rounded-lg">
                        <Send className="w-5 h-5" />
                    </button>
                </div>
            </form>
        </>
    );
}

export default App;
