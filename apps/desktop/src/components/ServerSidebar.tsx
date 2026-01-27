import { useState } from 'react';
import { Plus, MessageCircle } from 'lucide-react';
import { useAppStore } from '../store';

interface CreateServerModalProps {
    isOpen: boolean;
    onClose: () => void;
}

function CreateServerModal({ isOpen, onClose }: CreateServerModalProps) {
    const [mode, setMode] = useState<'create' | 'join'>('create');
    const [name, setName] = useState('');
    const [inviteCode, setInviteCode] = useState('');
    const [loading, setLoading] = useState(false);

    const createServer = useAppStore((s) => s.createServer);
    const joinServer = useAppStore((s) => s.joinServer);

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setLoading(true);
        try {
            if (mode === 'create') {
                await createServer(name);
            } else {
                await joinServer(inviteCode);
            }
            onClose();
            setName('');
            setInviteCode('');
        } finally {
            setLoading(false);
        }
    };

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50">
            <div className="bg-surface rounded-xl p-6 w-96 border border-white/10 shadow-2xl">
                <h2 className="text-xl font-bold mb-4 text-center">
                    {mode === 'create' ? 'Create a Server' : 'Join a Server'}
                </h2>

                <div className="flex rounded-lg bg-black/20 p-1 mb-4">
                    <button
                        onClick={() => setMode('create')}
                        className={`flex-1 py-2 text-sm font-medium rounded-md transition ${mode === 'create' ? 'bg-primary text-white' : 'text-gray-400 hover:text-white'
                            }`}
                    >
                        Create
                    </button>
                    <button
                        onClick={() => setMode('join')}
                        className={`flex-1 py-2 text-sm font-medium rounded-md transition ${mode === 'join' ? 'bg-primary text-white' : 'text-gray-400 hover:text-white'
                            }`}
                    >
                        Join
                    </button>
                </div>

                <form onSubmit={handleSubmit} className="space-y-4">
                    {mode === 'create' ? (
                        <input
                            type="text"
                            value={name}
                            onChange={(e) => setName(e.target.value)}
                            placeholder="Server name"
                            className="w-full px-4 py-3 bg-black/30 border border-white/10 rounded-lg focus:border-primary/50 outline-none text-white"
                            required
                        />
                    ) : (
                        <input
                            type="text"
                            value={inviteCode}
                            onChange={(e) => setInviteCode(e.target.value)}
                            placeholder="Invite code (e.g. ABC12345)"
                            className="w-full px-4 py-3 bg-black/30 border border-white/10 rounded-lg focus:border-primary/50 outline-none text-white"
                            required
                        />
                    )}

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
                            {loading ? 'Loading...' : mode === 'create' ? 'Create' : 'Join'}
                        </button>
                    </div>
                </form>
            </div>
        </div>
    );
}

export function ServerSidebar() {
    const [showModal, setShowModal] = useState(false);

    const servers = useAppStore((s) => s.servers);
    const activeServer = useAppStore((s) => s.activeServer);
    const setActiveServer = useAppStore((s) => s.setActiveServer);

    return (
        <>
            <div className="w-[72px] bg-[#0d0d12] flex flex-col items-center py-3 gap-2 border-r border-white/5">
                {/* DM Button */}
                <button
                    onClick={() => setActiveServer(null)}
                    className={`w-12 h-12 rounded-2xl flex items-center justify-center transition-all duration-200 ${activeServer === null
                            ? 'bg-primary rounded-xl'
                            : 'bg-surface hover:bg-primary/20 hover:rounded-xl'
                        }`}
                    title="Direct Messages"
                >
                    <MessageCircle className="w-5 h-5" />
                </button>

                <div className="w-8 h-0.5 bg-white/10 rounded-full my-1" />

                {/* Server List */}
                {servers.map((server) => (
                    <button
                        key={server.id}
                        onClick={() => setActiveServer(server.id)}
                        className={`w-12 h-12 rounded-2xl flex items-center justify-center transition-all duration-200 relative group ${activeServer === server.id
                                ? 'bg-primary rounded-xl'
                                : 'bg-surface hover:bg-primary/20 hover:rounded-xl'
                            }`}
                        title={server.name}
                    >
                        {server.icon_url ? (
                            <img
                                src={server.icon_url}
                                alt={server.name}
                                className="w-full h-full rounded-inherit object-cover"
                            />
                        ) : (
                            <span className="text-lg font-bold">
                                {server.name.charAt(0).toUpperCase()}
                            </span>
                        )}

                        {/* Active indicator */}
                        {activeServer === server.id && (
                            <div className="absolute left-0 w-1 h-8 bg-white rounded-r-full -translate-x-3" />
                        )}

                        {/* Tooltip */}
                        <div className="absolute left-full ml-4 px-3 py-2 bg-black rounded-lg text-sm font-medium whitespace-nowrap opacity-0 group-hover:opacity-100 pointer-events-none transition-opacity z-50">
                            {server.name}
                        </div>
                    </button>
                ))}

                {/* Add Server Button */}
                <button
                    onClick={() => setShowModal(true)}
                    className="w-12 h-12 rounded-2xl bg-surface hover:bg-green-500 hover:rounded-xl flex items-center justify-center transition-all duration-200 text-green-500 hover:text-white"
                    title="Add a Server"
                >
                    <Plus className="w-6 h-6" />
                </button>
            </div>

            <CreateServerModal isOpen={showModal} onClose={() => setShowModal(false)} />
        </>
    );
}
