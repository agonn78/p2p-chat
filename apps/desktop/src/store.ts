import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { User, Friend, Room, CallState } from './types';

const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:3000';

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
                const res = await fetch(`${API_URL}/auth/login`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ email, password }),
                });
                if (!res.ok) throw new Error('Login failed');
                const data = await res.json();
                set({ token: data.token, user: data.user, isAuthenticated: true });
            },

            register: async (username, email, password) => {
                const res = await fetch(`${API_URL}/auth/register`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username, email, password }),
                });
                if (!res.ok) {
                    const error = await res.json();
                    throw new Error(error.error || 'Registration failed');
                }
                const data = await res.json();
                set({ token: data.token, user: data.user, isAuthenticated: true });
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

            acceptFriend: async (friendshipId) => {
                const { token, fetchFriends, fetchPendingRequests } = get();
                if (!token) return;
                await fetch(`${API_URL}/friends/accept/${friendshipId}`, {
                    method: 'POST',
                    headers: { Authorization: `Bearer ${token}` },
                });
                await fetchFriends();
                await fetchPendingRequests();
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
