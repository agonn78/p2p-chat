import { useState } from 'react';
import { Hash, Volume2, Plus, Settings, ChevronDown, Mic, MicOff, LogOut, PhoneCall, PhoneOff } from 'lucide-react';
import { useAppStore } from '../store';
import type { Channel } from '../types';

interface CreateChannelModalProps {
    isOpen: boolean;
    onClose: () => void;
    serverId: string;
}

function CreateChannelModal({ isOpen, onClose, serverId }: CreateChannelModalProps) {
    const [name, setName] = useState('');
    const [type, setType] = useState<'text' | 'voice'>('text');
    const [loading, setLoading] = useState(false);

    const createChannel = useAppStore((s) => s.createChannel);

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setLoading(true);
        try {
            await createChannel(serverId, name, type);
            onClose();
            setName('');
        } finally {
            setLoading(false);
        }
    };

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50">
            <div className="bg-surface rounded-xl p-6 w-96 border border-white/10 shadow-2xl">
                <h2 className="text-xl font-bold mb-4">Create Channel</h2>

                <form onSubmit={handleSubmit} className="space-y-4">
                    <div className="flex gap-2">
                        <button
                            type="button"
                            onClick={() => setType('text')}
                            className={`flex-1 py-3 rounded-lg flex items-center justify-center gap-2 transition ${type === 'text' ? 'bg-primary' : 'bg-white/10 hover:bg-white/20'
                                }`}
                        >
                            <Hash className="w-4 h-4" />
                            Text
                        </button>
                        <button
                            type="button"
                            onClick={() => setType('voice')}
                            className={`flex-1 py-3 rounded-lg flex items-center justify-center gap-2 transition ${type === 'voice' ? 'bg-primary' : 'bg-white/10 hover:bg-white/20'
                                }`}
                        >
                            <Volume2 className="w-4 h-4" />
                            Voice
                        </button>
                    </div>

                    <input
                        type="text"
                        value={name}
                        onChange={(e) => setName(e.target.value.toLowerCase().replace(/\s+/g, '-'))}
                        placeholder="channel-name"
                        className="w-full px-4 py-3 bg-black/30 border border-white/10 rounded-lg focus:border-primary/50 outline-none text-white"
                        required
                    />

                    <div className="flex gap-2">
                        <button
                            type="button"
                            onClick={onClose}
                            className="flex-1 py-2 bg-white/10 hover:bg-white/20 rounded-lg transition"
                        >
                            Cancel
                        </button>
                        <button
                            type="submit"
                            disabled={loading}
                            className="flex-1 py-2 bg-primary hover:bg-primary/80 rounded-lg transition disabled:opacity-50"
                        >
                            {loading ? 'Creating...' : 'Create'}
                        </button>
                    </div>
                </form>
            </div>
        </div>
    );
}

export function ChannelList() {
    const [showCreateChannel, setShowCreateChannel] = useState(false);
    const [showInvite, setShowInvite] = useState(false);

    const servers = useAppStore((s) => s.servers);
    const activeServer = useAppStore((s) => s.activeServer);
    const channels = useAppStore((s) => s.channels);
    const activeChannel = useAppStore((s) => s.activeChannel);
    const setActiveChannel = useAppStore((s) => s.setActiveChannel);
    const voicePresenceByChannel = useAppStore((s) => s.voicePresenceByChannel);
    const activeVoiceChannel = useAppStore((s) => s.activeVoiceChannel);
    const joinVoiceChannel = useAppStore((s) => s.joinVoiceChannel);
    const leaveVoiceChannel = useAppStore((s) => s.leaveVoiceChannel);
    const user = useAppStore((s) => s.user);

    const currentServer = servers.find((s) => s.id === activeServer);
    const isOwner = currentServer?.owner_id === user?.id;

    const logout = useAppStore((s) => s.logout);
    const [isMuted, setIsMuted] = useState(false);

    const textChannels = channels.filter((c) => c.channel_type === 'text');
    const voiceChannels = channels.filter((c) => c.channel_type === 'voice');

    if (!activeServer || !currentServer) return null;

    const copyInviteCode = () => {
        navigator.clipboard.writeText(currentServer.invite_code);
        setShowInvite(true);
        setTimeout(() => setShowInvite(false), 2000);
    };

    return (
        <>
            <div className="w-72 bg-surface flex flex-col border-r border-white/5">
                {/* Server Header */}
                <div className="h-14 px-4 flex items-center justify-between border-b border-white/5 hover:bg-white/5 cursor-pointer">
                    <h2 className="font-bold truncate">{currentServer.name}</h2>
                    <ChevronDown className="w-4 h-4 text-gray-400" />
                </div>

                {/* Invite Code Banner */}
                <button
                    onClick={copyInviteCode}
                    className="mx-2 mt-2 px-3 py-2 bg-primary/20 hover:bg-primary/30 rounded-lg text-sm transition"
                >
                    {showInvite ? '✓ Copied!' : `Invite: ${currentServer.invite_code}`}
                </button>

                {/* Channel List */}
                <div className="flex-1 overflow-y-auto py-2">
                    {/* Text Channels */}
                    <div className="px-2">
                        <div className="flex items-center justify-between px-2 py-1.5 text-xs font-semibold text-gray-400 uppercase">
                            <span>Text Channels</span>
                            {isOwner && (
                                <button
                                    onClick={() => setShowCreateChannel(true)}
                                    className="hover:text-white transition"
                                >
                                    <Plus className="w-4 h-4" />
                                </button>
                            )}
                        </div>
                        {textChannels.map((channel) => (
                            <ChannelButton
                                key={channel.id}
                                channel={channel}
                                isActive={activeChannel === channel.id}
                                onClick={() => setActiveChannel(channel.id)}
                            />
                        ))}
                    </div>

                    {/* Voice Channels */}
                    {voiceChannels.length > 0 && (
                        <div className="px-2 mt-4">
                            <div className="flex items-center justify-between px-2 py-1.5 text-xs font-semibold text-gray-400 uppercase">
                                <span>Voice Channels</span>
                            </div>
                            {voiceChannels.map((channel) => (
                                <VoiceChannelButton
                                    key={channel.id}
                                    channel={channel}
                                    participantCount={(voicePresenceByChannel[channel.id] || []).length}
                                    joined={activeVoiceChannel === channel.id}
                                    onJoin={() => joinVoiceChannel(activeServer, channel.id)}
                                    onLeave={() => leaveVoiceChannel(activeServer, channel.id)}
                                />
                            ))}
                        </div>
                    )}
                </div>

                {/* User Status Footer */}
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
                                title={isMuted ? 'Unmute' : 'Mute'}
                            >
                                {isMuted ? <MicOff className="w-4 h-4" /> : <Mic className="w-4 h-4" />}
                            </button>
                            <button className="p-1.5 hover:bg-white/10 rounded text-gray-400" title="Settings">
                                <Settings className="w-4 h-4" />
                            </button>
                            <button onClick={logout} className="p-1.5 hover:bg-white/10 rounded text-gray-400" title="Logout">
                                <LogOut className="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            <CreateChannelModal
                isOpen={showCreateChannel}
                onClose={() => setShowCreateChannel(false)}
                serverId={activeServer}
            />
        </>
    );
}

function VoiceChannelButton({
    channel,
    participantCount,
    joined,
    onJoin,
    onLeave,
}: {
    channel: Channel;
    participantCount: number;
    joined: boolean;
    onJoin: () => void;
    onLeave: () => void;
}) {
    return (
        <div
            className={`w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm transition ${joined
                ? 'bg-green-500/15 text-green-300 border border-green-500/20'
                : 'text-gray-400 hover:text-white hover:bg-white/5'
                }`}
        >
            <Volume2 className="w-4 h-4 flex-shrink-0" />
            <span className="truncate flex-1">{channel.name}</span>
            <span className="text-xs text-gray-400">{participantCount}</span>
            <button
                onClick={joined ? onLeave : onJoin}
                className={`p-1 rounded transition ${joined ? 'hover:bg-red-500/20 text-red-300' : 'hover:bg-green-500/20 text-green-300'}`}
                title={joined ? 'Leave voice channel' : 'Join voice channel'}
            >
                {joined ? <PhoneOff className="w-4 h-4" /> : <PhoneCall className="w-4 h-4" />}
            </button>
        </div>
    );
}

function ChannelButton({
    channel,
    isActive,
    onClick,
}: {
    channel: Channel;
    isActive: boolean;
    onClick: () => void;
}) {
    return (
        <button
            onClick={onClick}
            className={`w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm transition ${isActive
                ? 'bg-white/10 text-white'
                : 'text-gray-400 hover:text-white hover:bg-white/5'
                }`}
        >
            {channel.channel_type === 'text' ? (
                <Hash className="w-4 h-4 flex-shrink-0" />
            ) : (
                <Volume2 className="w-4 h-4 flex-shrink-0" />
            )}
            <span className="truncate">{channel.name}</span>
        </button>
    );
}
