import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import type { User, Friend, Room, CallState, Message, Server, Channel, ServerMember, ChannelMessage, IncomingCallPayload, CallAcceptedPayload } from './types';
import * as crypto from './crypto';

// Note: All HTTP API calls now go through Rust Tauri commands (no CORS issues)

// Polling configuration
const POLL_INTERVAL_MS = 5000;  // Poll every 5 seconds when WS is down
const WS_RECONNECT_INTERVAL_MS = 10000;  // Try to reconnect WS every 10 seconds

// Check if a JWT token is expired
function isTokenExpired(token: string): boolean {
    try {
        const payload = JSON.parse(atob(token.split('.')[1]));
        return payload.exp * 1000 < Date.now();
    } catch {
        return true; // Treat invalid tokens as expired
    }
}

// Helper to handle API errors (auto-logout on 401)
function handleApiError(e: unknown, context: string, logoutFn?: () => void): void {
    const errStr = String(e);
    console.error(`[Store] ${context}:`, e);
    // Auto-logout if token is expired/invalid (Rust API returns error string containing status codes)
    if (logoutFn && (errStr.includes('401') || errStr.includes('Unauthorized') || errStr.includes('InvalidToken'))) {
        console.warn('[Store] ⚠️ Token expired or invalid, logging out...');
        logoutFn();
    }
}

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
    hasMoreMessages: boolean;
    isLoadingMoreMessages: boolean;
    unreadCounts: Record<string, number>;
    typingByRoom: Record<string, string[]>;
    typingByChannel: Record<string, string[]>;

    // Connection state
    wsConnected: boolean;
    lastMessageTimestamp: string | null;

    // Call
    activeCall: CallState | null;

    // Servers
    servers: Server[];
    activeServer: string | null;
    channels: Channel[];
    activeChannel: string | null;
    serverMembers: ServerMember[];
    channelMessages: ChannelMessage[];
    hasMoreChannelMessages: boolean;
    isLoadingMoreChannelMessages: boolean;

    // Actions
    login: (email: string, password: string) => Promise<void>;
    register: (username: string, email: string, password: string) => Promise<void>;
    logout: () => void;
    fetchFriends: () => Promise<void>;
    fetchPendingRequests: () => Promise<void>;
    sendFriendRequest: (username: string) => Promise<void>;
    acceptFriend: (friendId: string) => Promise<void>;
    setActiveRoom: (roomId: string | null) => void;
    startCall: (peerId: string) => Promise<void>;
    endCall: () => Promise<void>;
    handleIncomingCall: (payload: IncomingCallPayload) => void;
    acceptIncomingCall: () => Promise<void>;
    declineIncomingCall: () => Promise<void>;
    cancelOutgoingCall: () => Promise<void>;

    // Chat Actions
    createOrGetDm: (friendId: string) => Promise<void>;
    fetchMessages: (roomId: string, options?: { before?: string; append?: boolean; limit?: number }) => Promise<void>;
    loadOlderMessages: () => Promise<void>;
    sendMessage: (roomId: string, content: string) => Promise<void>;
    addMessage: (message: Message) => void;
    setTyping: (roomId: string, userId: string, isTyping: boolean) => void;
    setChannelTyping: (channelId: string, userId: string, isTyping: boolean) => void;
    removeMessage: (messageId: string) => void;
    deleteMessage: (messageId: string) => Promise<void>;
    deleteAllMessages: () => Promise<void>;
    editMessage: (messageId: string, content: string) => Promise<void>;
    updateMessage: (message: Message) => void;
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

    // Server Actions
    fetchServers: () => Promise<void>;
    createServer: (name: string, iconUrl?: string) => Promise<void>;
    joinServer: (inviteCode: string) => Promise<void>;
    leaveServer: (serverId: string) => Promise<void>;
    setActiveServer: (serverId: string | null) => void;
    fetchChannels: (serverId: string) => Promise<void>;
    createChannel: (serverId: string, name: string, type?: 'text' | 'voice') => Promise<void>;
    setActiveChannel: (channelId: string | null) => void;
    fetchServerMembers: (serverId: string) => Promise<void>;
    fetchChannelMessages: (serverId: string, channelId: string, options?: { before?: string; append?: boolean; limit?: number }) => Promise<void>;
    loadOlderChannelMessages: () => Promise<void>;
    sendChannelMessage: (serverId: string, channelId: string, content: string) => Promise<void>;
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
            hasMoreMessages: true,
            isLoadingMoreMessages: false,
            unreadCounts: {},
            typingByRoom: {},
            typingByChannel: {},
            wsConnected: false,
            lastMessageTimestamp: null,

            // Server state
            servers: [],
            activeServer: null,
            channels: [],
            activeChannel: null,
            serverMembers: [],
            channelMessages: [],
            hasMoreChannelMessages: true,
            isLoadingMoreChannelMessages: false,

            // Connection Actions
            setWsConnected: (connected) => {
                set({ wsConnected: connected });
                console.log(`[Store] WebSocket ${connected ? '🟢 connected' : '🔴 disconnected'}`);
            },

            // Auth actions
            login: async (email, password) => {
                console.log(`[Store] Attempting login via Rust API with email: ${email}`);
                try {
                    const data = await invoke<{ token: string; user: User }>('api_login', { email, password });
                    console.log('[Store] Login success, received token');
                    set({ token: data.token, user: data.user, isAuthenticated: true });

                    // Identify user on WebSocket for real-time messaging
                    try {
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
                console.log(`[Store] Attempting register via Rust API with user: ${username}`);
                try {
                    const data = await invoke<{ token: string; user: User }>('api_register', { username, email, password });
                    console.log('[Store] Register success, received token');
                    set({ token: data.token, user: data.user, isAuthenticated: true });

                    // Identify user on WebSocket
                    try {
                        await invoke('identify_user', { userId: data.user.id });
                        console.log('[Store] WebSocket identified with user ID:', data.user.id);
                    } catch (e) {
                        console.error('[Store] Failed to identify on WebSocket:', e);
                    }

                    // Initialize E2EE keys after registration
                    await get().initializeKeys();
                } catch (e) {
                    console.error('[Store] Register exception:', e);
                    throw e;
                }
            },

            logout: () => {
                crypto.clearSecretCache();
                // Also logout from Rust API state
                invoke('api_logout').catch(console.error);
                set({
                    token: null,
                    user: null,
                    isAuthenticated: false,
                    friends: [],
                    rooms: [],
                    messages: [],
                    hasMoreMessages: true,
                    isLoadingMoreMessages: false,
                    keyPair: null,
                    friendPublicKeys: {},
                    activeRoom: null,
                    activeFriendId: null,
                    channelMessages: [],
                    hasMoreChannelMessages: true,
                    isLoadingMoreChannelMessages: false,
                    typingByRoom: {},
                    typingByChannel: {},
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

                console.log('[Store] 🔐 Uploading public key to server via Rust API...');
                try {
                    await invoke('api_upload_public_key', { publicKey: keyPair.publicKey });
                    console.log('[Store] 🔐 Public key uploaded successfully');
                } catch (e) {
                    console.error('[Store] initializeKeys exception:', e);
                }
            },

            fetchFriendPublicKey: async (friendId) => {
                const { friendPublicKeys } = get();

                // Check cache first
                if (friendPublicKeys[friendId]) {
                    return friendPublicKeys[friendId];
                }

                console.log(`[Store] 🔐 Fetching public key for friend ${friendId}`);
                try {
                    const publicKey = await invoke<string | null>('api_fetch_user_public_key', { userId: friendId });
                    if (publicKey) {
                        set({
                            friendPublicKeys: {
                                ...get().friendPublicKeys,
                                [friendId]: publicKey
                            }
                        });
                        console.log(`[Store] 🔐 Got public key for friend ${friendId}`);
                        return publicKey;
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
                    console.log(`[E2EE-DEBUG] No cached key for ${friendId}, fetching...`);
                    fetchFriendPublicKey(friendId);
                    return '[Decrypting...]';
                }

                try {
                    console.log(`[E2EE-DEBUG] Decrypting message from ${message.sender_id}`);
                    console.log(`[E2EE-DEBUG] Using friendId: ${friendId}`);
                    console.log(`[E2EE-DEBUG] My public key: ${keyPair.publicKey?.substring(0, 20)}...`);
                    console.log(`[E2EE-DEBUG] Friend public key: ${friendPublicKey?.substring(0, 20)}...`);
                    console.log(`[E2EE-DEBUG] Nonce: ${message.nonce?.substring(0, 20)}...`);

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
                        console.log(`[E2EE-DEBUG] ✅ Decryption successful`);
                        return decrypted;
                    }
                    console.log(`[E2EE-DEBUG] ❌ decryptMessage returned null`);
                } catch (e) {
                    console.error('[Store] Decryption failed:', e);
                }

                return '[Encrypted]';
            },

            // Social actions
            fetchFriends: async () => {
                try {
                    const friends = await invoke<Friend[]>('api_fetch_friends');
                    set({ friends });

                    // Pre-fetch public keys for all friends
                    for (const friend of friends) {
                        get().fetchFriendPublicKey(friend.id);
                    }

                    // Also fetch online friends status (Bug #10)
                    try {
                        const onlineFriends = await invoke<string[]>('api_fetch_online_friends');
                        set({ onlineFriends });
                    } catch {
                        // Endpoint might not be wired up yet, don't fail
                        console.warn('[Store] Could not fetch online friends');
                    }
                } catch (e) {
                    handleApiError(e, 'fetchFriends', get().logout);
                }
            },

            fetchPendingRequests: async () => {
                try {
                    const pendingRequests = await invoke<Friend[]>('api_fetch_pending_requests');
                    set({ pendingRequests });
                } catch (e) {
                    console.error('[Store] fetchPendingRequests exception:', e);
                }
            },

            sendFriendRequest: async (username) => {
                try {
                    await invoke('api_send_friend_request', { username });
                } catch (e) {
                    console.error('[Store] sendFriendRequest exception:', e);
                    throw e;
                }
            },

            acceptFriend: async (friendId) => {
                const { fetchFriends, fetchPendingRequests } = get();
                console.log(`[Store] Accepting friend request from user ID: ${friendId}`);
                try {
                    await invoke('api_accept_friend', { friendId });
                    console.log('[Store] Friend accepted successfully. Refreshing lists...');
                    await fetchFriends();
                    await fetchPendingRequests();
                } catch (e) {
                    console.error('[Store] Accept exception:', e);
                }
            },

            // Chat actions
            createOrGetDm: async (friendId) => {
                const { markAsRead, fetchFriendPublicKey } = get();

                console.log(`[Store] Creating/Getting DM with friend: ${friendId}`);

                // Pre-fetch friend's public key for encryption
                await fetchFriendPublicKey(friendId);

                try {
                    const room = await invoke<Room>('api_create_or_get_dm', { friendId });
                    console.log(`[Store] Got room:`, room);

                    // Clear messages and set active room + friend
                    set({
                        messages: [],
                        activeRoom: room.id,
                        activeFriendId: friendId,
                        hasMoreMessages: true,
                        isLoadingMoreMessages: false,
                    });

                    // Mark messages as read
                    markAsRead(friendId);

                    // Fetch messages for this room
                    await get().fetchMessages(room.id, { limit: 100 });

                    // Immediately mark as read up to latest message for receipts
                    const latest = get().messages[get().messages.length - 1];
                    if (latest?.id) {
                        invoke('api_mark_room_read', { roomId: room.id, uptoMessageId: latest.id }).catch(() => undefined);
                    }
                } catch (e) {
                    console.error('[Store] createOrGetDm exception:', e);
                }
            },

            fetchMessages: async (roomId, options) => {
                const before = options?.before;
                const limit = options?.limit ?? 100;
                const append = options?.append ?? false;
                console.log(`[Store] Fetching messages for room: ${roomId} (before=${before ?? 'none'}, limit=${limit}, append=${append})`);

                if (append) {
                    set({ isLoadingMoreMessages: true });
                }

                try {
                    const fetched = await invoke<Message[]>('api_fetch_messages', { roomId, before, limit });
                    const normalized = fetched.map((m) => ({ ...m, status: m.status ?? 'sent' as const }));
                    console.log(`[Store] Fetched ${normalized.length} messages`);

                    if (append) {
                        const current = get().messages;
                        const merged = [...normalized, ...current].filter((msg, idx, arr) => {
                            if (arr.findIndex((x) => x.id === msg.id) !== idx) return false;
                            if (msg.client_id && arr.findIndex((x) => x.client_id === msg.client_id) !== idx) return false;
                            return true;
                        });
                        set({
                            messages: merged,
                            hasMoreMessages: normalized.length >= limit,
                            isLoadingMoreMessages: false,
                        });
                    } else {
                        set({
                            messages: normalized,
                            hasMoreMessages: normalized.length >= limit,
                            isLoadingMoreMessages: false,
                        });
                    }

                    if (normalized.length > 0) {
                        const lastMsg = normalized[normalized.length - 1];
                        set({ lastMessageTimestamp: lastMsg.created_at || null });
                    }
                } catch (e) {
                    console.error('[Store] fetchMessages exception:', e);
                    if (append) {
                        set({ isLoadingMoreMessages: false });
                    }
                }
            },

            loadOlderMessages: async () => {
                const { activeRoom, messages, hasMoreMessages, isLoadingMoreMessages, fetchMessages } = get();
                if (!activeRoom || !hasMoreMessages || isLoadingMoreMessages || messages.length === 0) {
                    return;
                }

                const firstMessage = messages.find((m) => !m.id.startsWith('local-'));
                if (!firstMessage) return;
                await fetchMessages(activeRoom, {
                    before: firstMessage.id,
                    append: true,
                    limit: 100,
                });
            },

            // Polling fallback for new messages
            pollForNewMessages: async () => {
                const { activeRoom, messages, addMessage } = get();
                if (!activeRoom) return;

                try {
                    const serverMessages = await invoke<Message[]>('api_fetch_messages', { roomId: activeRoom, limit: 100 });
                    const currentIds = new Set(messages.map(m => m.id));

                    // Add any new messages
                    for (const msg of serverMessages) {
                        if (!currentIds.has(msg.id)) {
                            console.log('[Store] 📬 Polling found new message:', msg.id);
                            addMessage(msg);
                        }
                    }
                } catch (e) {
                    console.error('[Store] pollForNewMessages exception:', e);
                }
            },

            sendMessage: async (roomId, content) => {
                const { keyPair, activeFriendId, friendPublicKeys, user, messages } = get();

                const clientId = globalThis.crypto.randomUUID();
                const localId = `local-${clientId}`;
                const nowIso = new Date().toISOString();

                let encryptedContent = content;
                let nonce: string | undefined = undefined;

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

                // Optimistic local message for status transitions
                const optimistic: Message = {
                    id: localId,
                    client_id: clientId,
                    room_id: roomId,
                    sender_id: user?.id,
                    content: encryptedContent,
                    nonce,
                    created_at: nowIso,
                    status: 'sending',
                    _decryptedContent: content,
                };
                set({ messages: [...messages, optimistic] });

                try {
                    const message = await invoke<Message>('api_send_message', {
                        roomId,
                        content: encryptedContent,
                        nonce,
                        clientId,
                    });
                    console.log('[Store] Message sent');
                    // Store original content for our own display
                    (message as Message & { _decryptedContent?: string })._decryptedContent = content;
                    message.status = message.status ?? 'sent';
                    set({
                        messages: get().messages.map((m) =>
                            m.id === localId || (m.client_id && m.client_id === clientId)
                                ? { ...message, _decryptedContent: content }
                                : m
                        )
                    });
                } catch (e) {
                    console.error('[Store] sendMessage exception:', e);
                    set({
                        messages: get().messages.map((m) =>
                            m.id === localId || (m.client_id && m.client_id === clientId)
                                ? { ...m, status: 'failed', _decryptedContent: content }
                                : m
                        )
                    });
                }
            },

            addMessage: (message) => {
                const { messages, activeRoom, user, friends, unreadCounts } = get();
                console.log(`[Store] addMessage called for room ${message.room_id}, active room: ${activeRoom}`);

                const existingIndex = messages.findIndex((m) =>
                    m.id === message.id ||
                    (!!message.client_id && !!m.client_id && m.client_id === message.client_id)
                );

                if (activeRoom === message.room_id) {
                    if (existingIndex >= 0) {
                        const next = [...messages];
                        next[existingIndex] = { ...next[existingIndex], ...message, status: message.status ?? next[existingIndex].status ?? 'sent' };
                        set({
                            messages: next,
                            lastMessageTimestamp: message.created_at || null,
                        });
                    } else {
                        console.log('[Store] Adding message to active conversation');
                        set({
                            messages: [...messages, { ...message, status: message.status ?? 'sent' }],
                            lastMessageTimestamp: message.created_at || null
                        });
                    }
                } else {
                    if (existingIndex >= 0) {
                        return;
                    }

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
            },

            setTyping: (roomId, userId, isTyping) => {
                const typingByRoom = { ...get().typingByRoom };
                const current = new Set(typingByRoom[roomId] || []);
                if (isTyping) {
                    current.add(userId);
                } else {
                    current.delete(userId);
                }
                typingByRoom[roomId] = Array.from(current);
                set({ typingByRoom });
            },

            setChannelTyping: (channelId, userId, isTyping) => {
                const typingByChannel = { ...get().typingByChannel };
                const current = new Set(typingByChannel[channelId] || []);
                if (isTyping) {
                    current.add(userId);
                } else {
                    current.delete(userId);
                }
                typingByChannel[channelId] = Array.from(current);
                set({ typingByChannel });
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

            editMessage: async (messageId, content) => {
                const { activeRoom, keyPair, activeFriendId, friendPublicKeys } = get();
                if (!activeRoom) return;

                if (messageId.startsWith('local-')) {
                    set({
                        messages: get().messages.map((m) =>
                            m.id === messageId ? { ...m, _decryptedContent: content, content } : m
                        )
                    });
                    return;
                }

                let encryptedContent = content;
                let nonce: string | undefined = undefined;

                // Encrypt if we have keys
                if (keyPair && activeFriendId && friendPublicKeys[activeFriendId]) {
                    const sharedSecret = crypto.getSharedSecret(
                        activeFriendId,
                        keyPair.secretKey,
                        friendPublicKeys[activeFriendId]
                    );
                    const encrypted = crypto.encryptMessage(content, sharedSecret);
                    encryptedContent = encrypted.ciphertext;
                    nonce = encrypted.nonce;
                }

                try {
                    const updated = await invoke<Message>('api_edit_message', {
                        roomId: activeRoom,
                        messageId,
                        content: encryptedContent,
                        nonce
                    });
                    updated._decryptedContent = content;
                    get().updateMessage(updated);
                } catch (e) {
                    console.error('[Store] editMessage exception:', e);
                }
            },

            updateMessage: (message) => {
                const { messages } = get();
                set({
                    messages: messages.map(m =>
                        m.id === message.id || (!!message.client_id && !!m.client_id && m.client_id === message.client_id)
                            ? { ...m, ...message }
                            : m
                    )
                });
            },

            deleteMessage: async (messageId) => {
                const { activeRoom, messages } = get();
                if (!activeRoom) return;

                if (messageId.startsWith('local-')) {
                    set({ messages: messages.filter(m => m.id !== messageId) });
                    return;
                }

                console.log(`[Store] 🗑️ Deleting message ${messageId}`);
                try {
                    await invoke('api_delete_message', { roomId: activeRoom, messageId });
                    set({ messages: messages.filter(m => m.id !== messageId) });
                    console.log('[Store] Message deleted successfully');
                } catch (e) {
                    console.error('[Store] deleteMessage exception:', e);
                }
            },

            deleteAllMessages: async () => {
                const { activeRoom } = get();
                if (!activeRoom) return;

                console.log(`[Store] 🗑️ Deleting ALL messages in room ${activeRoom}`);
                try {
                    await invoke('api_delete_all_messages', { roomId: activeRoom });
                    set({ messages: [] });
                    console.log('[Store] All messages deleted successfully');
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
            startCall: async (peerId) => {
                console.log('[CALL-DEBUG] ===== START CALL =====');
                console.log('[CALL-DEBUG] Target peerId:', peerId);
                const { friends } = get();
                const friend = friends.find(f => f.id === peerId);
                console.log('[CALL-DEBUG] Found friend:', friend?.username);

                const newCallState = {
                    status: 'calling' as const,
                    peerId,
                    peerName: friend?.username || 'Unknown',
                    peerPublicKey: null,
                    isMuted: false,
                    startTime: null,
                };
                console.log('[CALL-DEBUG] Setting activeCall state:', JSON.stringify(newCallState, null, 2));
                set({ activeCall: newCallState });

                try {
                    console.log('[CALL-DEBUG] Invoking start_call command...');
                    const result = await invoke('start_call', { targetId: peerId });
                    console.log('[CALL-DEBUG] ✅ start_call returned:', result);
                    console.log('[CALL-DEBUG] Call initiated, waiting for acceptance...');
                } catch (e) {
                    console.error('[CALL-DEBUG] ❌ Failed to start call:', e);
                    set({ activeCall: null });
                }
            },

            endCall: async () => {
                const { activeCall } = get();
                if (activeCall?.peerId) {
                    try {
                        await invoke('end_call', { peerId: activeCall.peerId });
                    } catch (e) {
                        console.error('[Call] Failed to end call:', e);
                    }
                }
                set({ activeCall: null });
            },

            handleIncomingCall: (payload) => {
                console.log('[Call] Incoming call from', payload.callerName);
                set({
                    activeCall: {
                        status: 'ringing',
                        peerId: payload.callerId,
                        peerName: payload.callerName,
                        peerPublicKey: payload.publicKey,
                        isMuted: false,
                        startTime: null,
                    },
                });
            },

            acceptIncomingCall: async () => {
                const { activeCall } = get();
                if (!activeCall || activeCall.status !== 'ringing') return;

                set({
                    activeCall: {
                        ...activeCall,
                        status: 'connecting',
                    },
                });

                try {
                    console.log('[CALL-DEBUG] Callee: Invoking accept_call...');
                    await invoke('accept_call', {
                        callerId: activeCall.peerId,
                        callerPublicKey: activeCall.peerPublicKey,
                    });
                    console.log('[CALL-DEBUG] Callee: ✅ accept_call succeeded');

                    console.log('[CALL-DEBUG] Callee: ✅ accept_call succeeded');

                    // Audio setup will be triggered by incoming WebRTC Offer (webrtc-offer event in App.tsx)


                    // E2EE established on callee side, now connected
                    set({
                        activeCall: {
                            ...activeCall,
                            status: 'connected',
                            startTime: Date.now(),
                        },
                    });
                    console.log('[CALL-DEBUG] Callee: ✅ Call status set to connected');
                } catch (e) {
                    console.error('[CALL-DEBUG] Callee: ❌ Failed to accept call:', e);
                    set({ activeCall: null });
                }
            },

            declineIncomingCall: async () => {
                const { activeCall } = get();
                if (!activeCall) return;

                try {
                    await invoke('decline_call', { callerId: activeCall.peerId });
                } catch (e) {
                    console.error('[Call] Failed to decline call:', e);
                }
                set({ activeCall: null });
            },

            cancelOutgoingCall: async () => {
                const { activeCall } = get();
                if (!activeCall || activeCall.status !== 'calling') return;

                try {
                    await invoke('cancel_call', { targetId: activeCall.peerId });
                } catch (e) {
                    console.error('[Call] Failed to cancel call:', e);
                }
                set({ activeCall: null });
            },

            // Server Actions
            fetchServers: async () => {
                try {
                    const servers = await invoke<Server[]>('api_fetch_servers');
                    set({ servers });
                    console.log(`[Store] Fetched ${servers.length} servers`);
                } catch (e) {
                    console.error('[Store] fetchServers error:', e);
                }
            },

            createServer: async (name, iconUrl) => {
                const { fetchServers } = get();
                try {
                    await invoke('api_create_server', { name, iconUrl });
                    console.log('[Store] Server created');
                    await fetchServers();
                } catch (e) {
                    console.error('[Store] createServer error:', e);
                }
            },

            joinServer: async (inviteCode) => {
                const { fetchServers } = get();
                try {
                    await invoke('api_join_server', { inviteCode });
                    console.log('[Store] Joined server');
                    await fetchServers();
                } catch (e) {
                    console.error('[Store] joinServer error:', e);
                }
            },

            leaveServer: async (serverId) => {
                const { fetchServers, activeServer } = get();
                try {
                    await invoke('api_leave_server', { serverId });
                    console.log('[Store] Left server');
                    if (activeServer === serverId) {
                        set({
                            activeServer: null,
                            channels: [],
                            activeChannel: null,
                            serverMembers: [],
                            channelMessages: [],
                            hasMoreChannelMessages: true,
                            isLoadingMoreChannelMessages: false,
                        });
                    }
                    await fetchServers();
                } catch (e) {
                    console.error('[Store] leaveServer error:', e);
                }
            },

            setActiveServer: (serverId) => {
                set({
                    activeServer: serverId,
                    activeChannel: null,
                    channels: [],
                    serverMembers: [],
                    channelMessages: [],
                    hasMoreChannelMessages: true,
                    isLoadingMoreChannelMessages: false,
                });
                if (serverId) {
                    get().fetchChannels(serverId);
                    get().fetchServerMembers(serverId);
                }
            },

            fetchChannels: async (serverId) => {
                try {
                    const data = await invoke<{ channels: Channel[] }>('api_fetch_server_details', { serverId });
                    set({ channels: data.channels });
                    console.log(`[Store] Fetched ${data.channels.length} channels`);

                    // Auto-select first text channel
                    const firstTextChannel = data.channels.find((c: Channel) => c.channel_type === 'text');
                    if (firstTextChannel) {
                        get().setActiveChannel(firstTextChannel.id);
                    }
                } catch (e) {
                    console.error('[Store] fetchChannels error:', e);
                }
            },

            createChannel: async (serverId, name, type = 'text') => {
                const { fetchChannels } = get();
                try {
                    await invoke('api_create_channel', { serverId, name, channelType: type });
                    console.log('[Store] Channel created');
                    await fetchChannels(serverId);
                } catch (e) {
                    console.error('[Store] createChannel error:', e);
                }
            },

            setActiveChannel: (channelId) => {
                const { activeServer } = get();
                set({
                    activeChannel: channelId,
                    channelMessages: [],
                    hasMoreChannelMessages: true,
                    isLoadingMoreChannelMessages: false,
                });
                if (channelId && activeServer) {
                    get().fetchChannelMessages(activeServer, channelId, { limit: 100 });
                }
            },

            fetchServerMembers: async (serverId) => {
                try {
                    const members = await invoke<ServerMember[]>('api_fetch_server_members', { serverId });
                    set({ serverMembers: members });
                    console.log(`[Store] Fetched ${members.length} members`);
                } catch (e) {
                    console.error('[Store] fetchServerMembers error:', e);
                }
            },

            fetchChannelMessages: async (serverId, channelId, options) => {
                const before = options?.before;
                const limit = options?.limit ?? 100;
                const append = options?.append ?? false;

                if (append) {
                    set({ isLoadingMoreChannelMessages: true });
                }

                try {
                    const fetched = await invoke<ChannelMessage[]>('api_fetch_channel_messages', { serverId, channelId, before, limit });
                    const normalized = fetched.map((m) => ({ ...m, status: m.status ?? 'sent' as const }));

                    if (append) {
                        const merged = [...normalized, ...get().channelMessages].filter((msg, idx, arr) => {
                            if (arr.findIndex((x) => x.id === msg.id) !== idx) return false;
                            if (msg.client_id && arr.findIndex((x) => x.client_id === msg.client_id) !== idx) return false;
                            return true;
                        });
                        set({
                            channelMessages: merged,
                            hasMoreChannelMessages: normalized.length >= limit,
                            isLoadingMoreChannelMessages: false,
                        });
                    } else {
                        set({
                            channelMessages: normalized,
                            hasMoreChannelMessages: normalized.length >= limit,
                            isLoadingMoreChannelMessages: false,
                        });
                    }

                    console.log(`[Store] Fetched ${normalized.length} channel messages`);
                } catch (e) {
                    console.error('[Store] fetchChannelMessages error:', e);
                    if (append) {
                        set({ isLoadingMoreChannelMessages: false });
                    }
                }
            },

            loadOlderChannelMessages: async () => {
                const {
                    activeServer,
                    activeChannel,
                    channelMessages,
                    hasMoreChannelMessages,
                    isLoadingMoreChannelMessages,
                    fetchChannelMessages,
                } = get();

                if (!activeServer || !activeChannel || !hasMoreChannelMessages || isLoadingMoreChannelMessages || channelMessages.length === 0) {
                    return;
                }

                const first = channelMessages.find((m) => !m.id.startsWith('local-'));
                if (!first) return;
                await fetchChannelMessages(activeServer, activeChannel, {
                    before: first.id,
                    append: true,
                    limit: 100,
                });
            },

            sendChannelMessage: async (serverId, channelId, content) => {
                const { channelMessages, user } = get();
                const clientId = globalThis.crypto.randomUUID();
                const localId = `local-${clientId}`;
                const optimistic: ChannelMessage = {
                    id: localId,
                    client_id: clientId,
                    channel_id: channelId,
                    sender_id: user?.id,
                    sender_username: user?.username,
                    content,
                    nonce: null,
                    created_at: new Date().toISOString(),
                    status: 'sending',
                };
                set({ channelMessages: [...channelMessages, optimistic] });

                try {
                    const message = await invoke<ChannelMessage>('api_send_channel_message', { serverId, channelId, content, clientId });
                    set({
                        channelMessages: get().channelMessages.map((m) =>
                            m.id === localId || (m.client_id && m.client_id === clientId)
                                ? { ...message, status: message.status ?? 'sent' }
                                : m
                        )
                    });
                    console.log('[Store] Channel message sent');
                } catch (e) {
                    console.error('[Store] sendChannelMessage error:', e);
                    set({
                        channelMessages: get().channelMessages.map((m) =>
                            m.id === localId || (m.client_id && m.client_id === clientId)
                                ? { ...m, status: 'failed' }
                                : m
                        )
                    });
                }
            },
        }),
        {
            name: 'p2p-nitro-store',
            partialize: (state) => ({
                token: state.token,
                user: state.user,
                keyPair: state.keyPair,
                friendPublicKeys: state.friendPublicKeys,
            }),
            onRehydrateStorage: (_state) => {
                // Bug #13: Validate token on app reload
                return (state?: AppState) => {
                    if (state?.token && isTokenExpired(state.token)) {
                        console.warn('[Store] ⚠️ Persisted token is expired, logging out...');
                        state.logout();
                    }
                };
            },
        }
    )
);

// Export polling interval for use in App.tsx
export { POLL_INTERVAL_MS, WS_RECONNECT_INTERVAL_MS };
