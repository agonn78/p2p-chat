import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { User, Friend, Room, CallState, Message } from './types';
import * as crypto from './crypto';

const API_URL = import.meta.env.VITE_API_URL || 'http://192.168.0.52:3000';

// Polling configuration
const POLL_INTERVAL_MS = 5000;  // Poll every 5 seconds when WS is down
const WS_RECONNECT_INTERVAL_MS = 10000;  // Try to reconnect WS every 10 seconds

interface AppState {
    // Auth
    token: string | null;
    user: User | null;
    isAuthenticated: boolean;

    // E2EE Keys
    keyPair: crypto.KeyPair | null;
    friendPublicKeys: Record<string, string>; // friendId -> base64 public key

    // Social
    friends: Friend[];
    pendingRequests: Friend[];
    onlineFriends: string[];

    // Rooms & Messages
    rooms: Room[];
    activeRoom: string | null;
    activeFriendId: string | null;
    messages: Message[];
    unreadCounts: Record<string, number>;

    // Connection state
    wsConnected: boolean;
    lastMessageTimestamp: string | null;

    // Call
    activeCall: CallState | null;

    // Actions
    login: (email: string, password: string) => Promise<void>;
    register: (username: string, email: string, password: string) => Promise<void>;
    logout: () => void;
    fetchFriends: () => Promise<void>;
    fetchPendingRequests: () => Promise<void>;
    sendFriendRequest: (username: string) => Promise<void>;
    acceptFriend: (friendId: string) => Promise<void>;
    setActiveRoom: (roomId: string | null) => void;
    startCall: (peerId: string) => void;
    endCall: () => void;

    // Chat Actions
    createOrGetDm: (friendId: string) => Promise<void>;
    fetchMessages: (roomId: string) => Promise<void>;
    sendMessage: (roomId: string, content: string) => Promise<void>;
    addMessage: (message: Message) => void;
    removeMessage: (messageId: string) => void;
    deleteMessage: (messageId: string) => Promise<void>;
    deleteAllMessages: () => Promise<void>;
    clearMessages: () => void;
    markAsRead: (friendId: string) => void;
    getUnreadCount: (friendId: string) => number;
    pollForNewMessages: () => Promise<void>;

    // E2EE Actions
    initializeKeys: () => Promise<void>;
    fetchFriendPublicKey: (friendId: string) => Promise<string | null>;
    decryptMessageContent: (message: Message) => string;

    // Connection Actions
    setWsConnected: (connected: boolean) => void;
}

export const useAppStore = create<AppState>()(
    persist(
        (set, get) => ({
            // Initial state
            token: null,
            user: null,
            isAuthenticated: false,
            keyPair: null,
            friendPublicKeys: {},
            friends: [],
            pendingRequests: [],
            onlineFriends: [],
            rooms: [],
            activeRoom: null,
            activeFriendId: null,
            activeCall: null,
            messages: [],
            unreadCounts: {},
            wsConnected: false,
            lastMessageTimestamp: null,

            // Connection Actions
            setWsConnected: (connected) => {
                set({ wsConnected: connected });
                console.log(`[Store] WebSocket ${connected ? '🟢 connected' : '🔴 disconnected'}`);
            },

            // Auth actions
            login: async (email, password) => {
                console.log(`[Store] Attempting login to ${API_URL}/auth/login with email: ${email}`);
                try {
                    const res = await fetch(`${API_URL}/auth/login`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ email, password }),
                    });

                    console.log(`[Store] Login response status: ${res.status}`);

                    if (!res.ok) {
                        const errorText = await res.text();
                        console.error('[Store] Login failed body:', errorText);
                        throw new Error(`Login failed: ${res.status} ${errorText}`);
                    }

                    const data = await res.json();
                    console.log('[Store] Login success, received token');
                    set({ token: data.token, user: data.user, isAuthenticated: true });

                    // Identify user on WebSocket for real-time messaging
                    try {
                        const { invoke } = await import('@tauri-apps/api/core');
                        await invoke('identify_user', { userId: data.user.id });
                        console.log('[Store] WebSocket identified with user ID:', data.user.id);
                    } catch (e) {
                        console.error('[Store] Failed to identify on WebSocket:', e);
                    }

                    // Initialize E2EE keys after login
                    await get().initializeKeys();
                } catch (e) {
                    console.error('[Store] Login exception:', e);
                    throw e;
                }
            },

            register: async (username, email, password) => {
                console.log(`[Store] Attempting register to ${API_URL}/auth/register with user: ${username}`);
                try {
                    const res = await fetch(`${API_URL}/auth/register`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ username, email, password }),
                    });

                    console.log(`[Store] Register response status: ${res.status}`);

                    if (!res.ok) {
                        const error = await res.json();
                        console.error('[Store] Register failed:', error);
                        throw new Error(error.error || 'Registration failed');
                    }
                    const data = await res.json();
                    console.log('[Store] Register success, received token');
                    set({ token: data.token, user: data.user, isAuthenticated: true });

                    // Initialize E2EE keys after registration
                    await get().initializeKeys();
                } catch (e) {
                    console.error('[Store] Register exception:', e);
                    throw e;
                }
            },

            logout: () => {
                crypto.clearSecretCache();
                set({
                    token: null,
                    user: null,
                    isAuthenticated: false,
                    friends: [],
                    rooms: [],
                    messages: [],
                    keyPair: null,
                    friendPublicKeys: {},
                    activeRoom: null,
                    activeFriendId: null,
                    wsConnected: false,
                });
            },

            // E2EE Actions
            initializeKeys: async () => {
                const { token } = get();
                if (!token) return;

                // Get or create keypair
                const keyPair = crypto.getOrCreateKeyPair();
                set({ keyPair });

                console.log('[Store] 🔐 Uploading public key to server...');
                try {
                    const res = await fetch(`${API_URL}/users/me/public-key`, {
                        method: 'PUT',
                        headers: {
                            'Content-Type': 'application/json',
                            Authorization: `Bearer ${token}`
                        },
                        body: JSON.stringify({ public_key: keyPair.publicKey }),
                    });

                    if (res.ok) {
                        console.log('[Store] 🔐 Public key uploaded successfully');
                    } else {
                        console.error('[Store] Failed to upload public key:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] initializeKeys exception:', e);
                }
            },

            fetchFriendPublicKey: async (friendId) => {
                const { token, friendPublicKeys } = get();
                if (!token) return null;

                // Check cache first
                if (friendPublicKeys[friendId]) {
                    return friendPublicKeys[friendId];
                }

                console.log(`[Store] 🔐 Fetching public key for friend ${friendId}`);
                try {
                    const res = await fetch(`${API_URL}/users/${friendId}/public-key`, {
                        headers: { Authorization: `Bearer ${token}` },
                    });

                    if (res.ok) {
                        const data = await res.json();
                        if (data.public_key) {
                            set({
                                friendPublicKeys: {
                                    ...get().friendPublicKeys,
                                    [friendId]: data.public_key
                                }
                            });
                            console.log(`[Store] 🔐 Got public key for friend ${friendId}`);
                            return data.public_key;
                        }
                    }
                } catch (e) {
                    console.error('[Store] fetchFriendPublicKey exception:', e);
                }
                return null;
            },

            decryptMessageContent: (message) => {
                const { keyPair, user, friendPublicKeys, friends, activeFriendId, fetchFriendPublicKey } = get();

                // If no encryption (legacy message)
                if (!message.nonce || !keyPair) {
                    return message.content;
                }

                // Determine whose public key to use based on sender
                // If I sent it, I need the friend's public key (use activeFriendId)
                // If friend sent it, I need their public key (use sender_id)
                let friendId: string | null = null;

                if (message.sender_id === user?.id) {
                    // I sent this message - use the active friend's key
                    friendId = activeFriendId;
                } else {
                    // Friend sent this message - use sender's key
                    friendId = message.sender_id || null;
                }

                if (!friendId) {
                    console.warn('[Store] Cannot determine friend ID for decryption');
                    return '[Encrypted]';
                }

                const friendPublicKey = friendPublicKeys[friendId];
                if (!friendPublicKey) {
                    // Try to fetch the key asynchronously
                    fetchFriendPublicKey(friendId);
                    return '[Decrypting...]';
                }

                try {
                    const sharedSecret = crypto.getSharedSecret(
                        friendId,
                        keyPair.secretKey,
                        friendPublicKey
                    );

                    const decrypted = crypto.decryptMessage(
                        message.content,
                        message.nonce,
                        sharedSecret
                    );

                    if (decrypted) {
                        return decrypted;
                    }
                } catch (e) {
                    console.error('[Store] Decryption failed:', e);
                }

                return '[Encrypted]';
            },

            // Social actions
            fetchFriends: async () => {
                const { token } = get();
                if (!token) return;
                const res = await fetch(`${API_URL}/friends`, {
                    headers: { Authorization: `Bearer ${token}` },
                });
                if (res.ok) {
                    const friends = await res.json();
                    set({ friends });

                    // Pre-fetch public keys for all friends
                    for (const friend of friends) {
                        get().fetchFriendPublicKey(friend.id);
                    }
                }
            },

            fetchPendingRequests: async () => {
                const { token } = get();
                if (!token) return;
                const res = await fetch(`${API_URL}/friends/pending`, {
                    headers: { Authorization: `Bearer ${token}` },
                });
                if (res.ok) {
                    const pendingRequests = await res.json();
                    set({ pendingRequests });
                }
            },

            sendFriendRequest: async (username) => {
                const { token } = get();
                if (!token) return;
                await fetch(`${API_URL}/friends/request`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        Authorization: `Bearer ${token}`,
                    },
                    body: JSON.stringify({ username }),
                });
            },

            acceptFriend: async (friendId) => {
                const { token, fetchFriends, fetchPendingRequests } = get();
                if (!token) return;
                console.log(`[Store] Accepting friend request from user ID: ${friendId}`);
                try {
                    const res = await fetch(`${API_URL}/friends/accept/${friendId}`, {
                        method: 'POST',
                        headers: { Authorization: `Bearer ${token}` },
                    });

                    console.log(`[Store] Accept response status: ${res.status}`);

                    if (!res.ok) {
                        const errorText = await res.text();
                        console.error('[Store] Accept failed body:', errorText);
                        throw new Error(`Accept failed: ${res.status} ${errorText}`);
                    }

                    console.log('[Store] Friend accepted successfully. Refreshing lists...');
                    await fetchFriends();
                    await fetchPendingRequests();
                } catch (e) {
                    console.error('[Store] Accept exception:', e);
                }
            },

            // Chat actions
            createOrGetDm: async (friendId) => {
                const { token, markAsRead, fetchFriendPublicKey } = get();
                if (!token) return;

                console.log(`[Store] Creating/Getting DM with friend: ${friendId}`);

                // Pre-fetch friend's public key for encryption
                await fetchFriendPublicKey(friendId);

                try {
                    const res = await fetch(`${API_URL}/chat/dm`, {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            Authorization: `Bearer ${token}`
                        },
                        body: JSON.stringify({ friend_id: friendId }),
                    });

                    if (res.ok) {
                        const room = await res.json();
                        console.log(`[Store] Got room:`, room);

                        // Clear messages and set active room + friend
                        set({ messages: [], activeRoom: room.id, activeFriendId: friendId });

                        // Mark messages as read
                        markAsRead(friendId);

                        // Fetch messages for this room
                        await get().fetchMessages(room.id);
                    } else {
                        console.error('[Store] Failed to create DM:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] createOrGetDm exception:', e);
                }
            },

            fetchMessages: async (roomId) => {
                const { token } = get();
                if (!token) return;

                console.log(`[Store] Fetching messages for room: ${roomId}`);
                try {
                    const res = await fetch(`${API_URL}/chat/${roomId}/messages`, {
                        headers: { Authorization: `Bearer ${token}` },
                    });

                    if (res.ok) {
                        const messages = await res.json();
                        console.log(`[Store] Fetched ${messages.length} messages`);
                        set({ messages });

                        // Update last message timestamp for polling
                        if (messages.length > 0) {
                            const lastMsg = messages[messages.length - 1];
                            set({ lastMessageTimestamp: lastMsg.created_at });
                        }
                    } else {
                        console.error('[Store] Failed to fetch messages:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] fetchMessages exception:', e);
                }
            },

            // Polling fallback for new messages
            pollForNewMessages: async () => {
                const { token, activeRoom, messages, addMessage } = get();
                if (!token || !activeRoom) return;

                try {
                    const res = await fetch(`${API_URL}/chat/${activeRoom}/messages`, {
                        headers: { Authorization: `Bearer ${token}` },
                    });

                    if (res.ok) {
                        const serverMessages: Message[] = await res.json();
                        const currentIds = new Set(messages.map(m => m.id));

                        // Add any new messages
                        for (const msg of serverMessages) {
                            if (!currentIds.has(msg.id)) {
                                console.log('[Store] 📬 Polling found new message:', msg.id);
                                addMessage(msg);
                            }
                        }
                    }
                } catch (e) {
                    console.error('[Store] pollForNewMessages exception:', e);
                }
            },

            sendMessage: async (roomId, content) => {
                const { token, keyPair, activeFriendId, friendPublicKeys } = get();
                if (!token) return;

                let encryptedContent = content;
                let nonce: string | null = null;

                // Encrypt if we have keys
                if (keyPair && activeFriendId && friendPublicKeys[activeFriendId]) {
                    console.log('[Store] 🔐 Encrypting message...');
                    const sharedSecret = crypto.getSharedSecret(
                        activeFriendId,
                        keyPair.secretKey,
                        friendPublicKeys[activeFriendId]
                    );

                    const encrypted = crypto.encryptMessage(content, sharedSecret);
                    encryptedContent = encrypted.ciphertext;
                    nonce = encrypted.nonce;
                    console.log('[Store] 🔐 Message encrypted');
                } else {
                    console.warn('[Store] ⚠️ Sending unencrypted message (no keys available)');
                }

                console.log(`[Store] Sending message to room ${roomId}`);
                try {
                    const res = await fetch(`${API_URL}/chat/${roomId}/messages`, {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            Authorization: `Bearer ${token}`
                        },
                        body: JSON.stringify({ content: encryptedContent, nonce }),
                    });

                    if (res.ok) {
                        const message = await res.json();
                        console.log('[Store] Message sent');
                        // Store original content for our own display
                        message._decryptedContent = content;
                        get().addMessage(message);
                    } else {
                        console.error('[Store] Failed to send message:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] sendMessage exception:', e);
                }
            },

            addMessage: (message) => {
                const { messages, activeRoom, user, friends, unreadCounts } = get();
                console.log(`[Store] addMessage called for room ${message.room_id}, active room: ${activeRoom}`);

                if (!messages.find(m => m.id === message.id)) {
                    if (activeRoom === message.room_id) {
                        console.log('[Store] Adding message to active conversation');
                        set({
                            messages: [...messages, message],
                            lastMessageTimestamp: message.created_at
                        });
                    } else {
                        console.log('[Store] Message for different room, incrementing unread');

                        const senderId = message.sender_id;
                        if (senderId && senderId !== user?.id) {
                            const friend = friends.find(f => f.id === senderId);
                            if (friend) {
                                const currentCount = unreadCounts[friend.id] || 0;
                                set({
                                    unreadCounts: {
                                        ...unreadCounts,
                                        [friend.id]: currentCount + 1
                                    }
                                });
                                console.log(`[Store] Unread count for ${friend.username}: ${currentCount + 1}`);
                            }
                        }
                    }
                } else {
                    console.log('[Store] Message not added (duplicate)');
                }
            },

            markAsRead: (friendId) => {
                const { unreadCounts } = get();
                if (unreadCounts[friendId]) {
                    const newCounts = { ...unreadCounts };
                    delete newCounts[friendId];
                    set({ unreadCounts: newCounts });
                    console.log(`[Store] Marked messages from ${friendId} as read`);
                }
            },

            getUnreadCount: (friendId) => {
                return get().unreadCounts[friendId] || 0;
            },

            // Delete actions
            removeMessage: (messageId) => {
                const { messages } = get();
                set({ messages: messages.filter(m => m.id !== messageId) });
                console.log(`[Store] Removed message ${messageId} from view`);
            },

            deleteMessage: async (messageId) => {
                const { token, activeRoom, messages } = get();
                if (!token || !activeRoom) return;

                console.log(`[Store] 🗑️ Deleting message ${messageId}`);
                try {
                    const res = await fetch(`${API_URL}/chat/${activeRoom}/messages/${messageId}`, {
                        method: 'DELETE',
                        headers: { Authorization: `Bearer ${token}` },
                    });

                    if (res.ok) {
                        set({ messages: messages.filter(m => m.id !== messageId) });
                        console.log('[Store] Message deleted successfully');
                    } else {
                        console.error('[Store] Failed to delete message:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] deleteMessage exception:', e);
                }
            },

            deleteAllMessages: async () => {
                const { token, activeRoom } = get();
                if (!token || !activeRoom) return;

                console.log(`[Store] 🗑️ Deleting ALL messages in room ${activeRoom}`);
                try {
                    const res = await fetch(`${API_URL}/chat/${activeRoom}/messages`, {
                        method: 'DELETE',
                        headers: { Authorization: `Bearer ${token}` },
                    });

                    if (res.ok) {
                        set({ messages: [] });
                        console.log('[Store] All messages deleted successfully');
                    } else {
                        console.error('[Store] Failed to delete all messages:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] deleteAllMessages exception:', e);
                }
            },

            clearMessages: () => {
                set({ messages: [] });
                console.log('[Store] Cleared messages from view (local only)');
            },

            // Room actions
            setActiveRoom: (roomId) => set({ activeRoom: roomId }),

            // Call actions
            startCall: (peerId) => {
                set({
                    activeCall: {
                        roomId: peerId,
                        peerId,
                        isConnected: false,
                        isMuted: false,
                        isVideoEnabled: false,
                    },
                });
            },

            endCall: () => set({ activeCall: null }),
        }),
        {
            name: 'p2p-nitro-store',
            partialize: (state) => ({
                token: state.token,
                user: state.user,
                keyPair: state.keyPair,
                friendPublicKeys: state.friendPublicKeys,
            }),
        }
    )
);

// Export polling interval for use in App.tsx
export { POLL_INTERVAL_MS, WS_RECONNECT_INTERVAL_MS };
