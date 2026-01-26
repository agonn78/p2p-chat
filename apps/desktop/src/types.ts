export interface User {
    id: string;
    username: string;
    avatar_url?: string;
    last_seen?: string;
}

export interface Friend extends User {
    status: string;
}

export interface AuthResponse {
    token: string;
    user: User;
}

export interface Room {
    id: string;
    name?: string;
    is_dm: boolean;
}

export interface Message {
    id: string;
    room_id: string;
    sender_id?: string;
    content: string;
    nonce?: string | null;  // For E2EE - if present, content is encrypted
    created_at: string;
    _decryptedContent?: string;  // Client-side only - cached decrypted content
}

export interface CallState {
    roomId: string;
    peerId: string;
    isConnected: boolean;
    isMuted: boolean;
    isVideoEnabled: boolean;
}
