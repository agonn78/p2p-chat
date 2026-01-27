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

// === Voice Call Types ===

export type CallStatus = 'idle' | 'calling' | 'ringing' | 'connecting' | 'connected' | 'ended';

export interface CallState {
    status: CallStatus;
    peerId: string | null;
    peerName: string | null;
    peerPublicKey: string | null;
    isMuted: boolean;
    startTime: number | null;
}

export interface IncomingCallPayload {
    callerId: string;
    callerName: string;
    publicKey: string;
}

export interface CallAcceptedPayload {
    peerId: string;
    publicKey: string;
}

// Server types
export interface Server {
    id: string;
    name: string;
    icon_url: string | null;
    owner_id: string;
    invite_code: string;
    created_at: string;
}

export interface Channel {
    id: string;
    server_id: string;
    name: string;
    channel_type: 'text' | 'voice';
    position: number;
    created_at: string;
}

export interface ServerMember {
    user_id: string;
    username: string;
    avatar_url: string | null;
    role: 'owner' | 'admin' | 'member';
    last_seen: string | null;
}

export interface ChannelMessage {
    id: string;
    channel_id: string;
    sender_id?: string;
    content: string;
    nonce?: string | null;
    created_at: string;
    _decryptedContent?: string;
}

