import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { User, Friend, Room, CallState } from './types';

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

    // Call
    activeCall: CallState | null;

    // Actions
    login: (email: string, password: string) => Promise<void>;
    register: (username: string, email: string, password: string) => Promise<void>;
    logout: () => void;
    fetchFriends: () => Promise<void>;
    fetchPendingRequests: () => Promise<void>;
    sendFriendRequest: (username: string) => Promise<void>;
    acceptFriend: (friendshipId: string) => Promise<void>;
    setActiveRoom: (roomId: string | null) => void;
    startCall: (peerId: string) => void;
    endCall: () => void;
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
                set({ token: null, user: null, isAuthenticated: false, friends: [], rooms: [] });
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
