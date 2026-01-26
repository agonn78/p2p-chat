import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { User, Friend, Room, CallState, Message } from './types';

const API_URL = import.meta.env.VITE_API_URL || 'http://192.168.0.52:3000';

interface AppState {
    // Auth
    token: string | null;
    user: User | null;
    isAuthenticated: boolean;

    // Social
    friends: Friend[];
    pendingRequests: Friend[];
    onlineFriends: string[];

    // Rooms & Messages
    rooms: Room[];
    activeRoom: string | null;
    messages: Message[];

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
}

export const useAppStore = create<AppState>()(
    persist(
        (set, get) => ({
            // Initial state
            token: null,
            user: null,
            isAuthenticated: false,
            friends: [],
            pendingRequests: [],
            onlineFriends: [],
            rooms: [],
            activeRoom: null,
            activeCall: null,
            messages: [],

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
                } catch (e) {
                    console.error('[Store] Register exception:', e);
                    throw e;
                }
            },

            logout: () => {
                set({ token: null, user: null, isAuthenticated: false, friends: [], rooms: [], messages: [] });
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
                const { token } = get();
                if (!token) return;

                console.log(`[Store] Creating/Getting DM with friend: ${friendId}`);
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
                        set({ activeRoom: room.id });
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
                    } else {
                        console.error('[Store] Failed to fetch messages:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] fetchMessages exception:', e);
                }
            },

            sendMessage: async (roomId, content) => {
                const { token } = get();
                if (!token) return;

                console.log(`[Store] Sending message to room ${roomId}: "${content}"`);
                try {
                    const res = await fetch(`${API_URL}/chat/${roomId}/messages`, {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            Authorization: `Bearer ${token}`
                        },
                        body: JSON.stringify({ content }),
                    });

                    if (res.ok) {
                        const message = await res.json();
                        console.log('[Store] Message sent, adding to list');
                        get().addMessage(message);
                    } else {
                        console.error('[Store] Failed to send message:', await res.text());
                    }
                } catch (e) {
                    console.error('[Store] sendMessage exception:', e);
                }
            },

            addMessage: (message) => {
                const { messages, activeRoom } = get();
                console.log(`[Store] addMessage called for room ${message.room_id}, active room: ${activeRoom}`);
                // Only add if it belongs to active room and not already there
                if (activeRoom === message.room_id && !messages.find(m => m.id === message.id)) {
                    console.log('[Store] Adding message to list');
                    set({ messages: [...messages, message] });
                } else {
                    console.log('[Store] Message not added (wrong room or duplicate)');
                }
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
            partialize: (state) => ({ token: state.token, user: state.user }),
        }
    )
);
