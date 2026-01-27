import { Crown, Shield } from 'lucide-react';
import { useAppStore } from '../store';

export function MemberList() {
    const serverMembers = useAppStore((s) => s.serverMembers);
    const activeServer = useAppStore((s) => s.activeServer);

    if (!activeServer) return null;

    // Group members by role
    const owners = serverMembers.filter((m) => m.role === 'owner');
    const admins = serverMembers.filter((m) => m.role === 'admin');
    const members = serverMembers.filter((m) => m.role === 'member');

    // Check online status (within last 5 minutes)
    const isOnline = (lastSeen: string | null) => {
        if (!lastSeen) return false;
        const diff = Date.now() - new Date(lastSeen).getTime();
        return diff < 5 * 60 * 1000;
    };

    const onlineMembers = serverMembers.filter((m) => isOnline(m.last_seen));
    const offlineMembers = serverMembers.filter((m) => !isOnline(m.last_seen));

    return (
        <div className="w-60 bg-surface border-l border-white/5 flex flex-col">
            <div className="p-4 border-b border-white/5">
                <h3 className="text-sm font-semibold text-gray-400 uppercase">
                    Members — {serverMembers.length}
                </h3>
            </div>

            <div className="flex-1 overflow-y-auto p-2">
                {/* Online Members */}
                {onlineMembers.length > 0 && (
                    <div className="mb-4">
                        <div className="px-2 py-1.5 text-xs font-semibold text-gray-400 uppercase">
                            Online — {onlineMembers.length}
                        </div>
                        {onlineMembers.map((member) => (
                            <MemberItem key={member.user_id} member={member} isOnline={true} />
                        ))}
                    </div>
                )}

                {/* Offline Members */}
                {offlineMembers.length > 0 && (
                    <div>
                        <div className="px-2 py-1.5 text-xs font-semibold text-gray-400 uppercase">
                            Offline — {offlineMembers.length}
                        </div>
                        {offlineMembers.map((member) => (
                            <MemberItem key={member.user_id} member={member} isOnline={false} />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

function MemberItem({
    member,
    isOnline,
}: {
    member: { user_id: string; username: string; avatar_url: string | null; role: string };
    isOnline: boolean;
}) {
    const getRoleIcon = () => {
        switch (member.role) {
            case 'owner':
                return <Crown className="w-3 h-3 text-yellow-400" />;
            case 'admin':
                return <Shield className="w-3 h-3 text-blue-400" />;
            default:
                return null;
        }
    };

    return (
        <div
            className={`flex items-center gap-2 px-2 py-1.5 rounded hover:bg-white/5 cursor-pointer ${!isOnline ? 'opacity-50' : ''
                }`}
        >
            <div className="relative">
                <div className="w-8 h-8 rounded-full bg-gradient-to-br from-primary to-secondary" />
                <div
                    className={`absolute bottom-0 right-0 w-3 h-3 rounded-full border-2 border-surface ${isOnline ? 'bg-green-500' : 'bg-gray-500'
                        }`}
                />
            </div>
            <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1">
                    <span className="text-sm truncate">{member.username}</span>
                    {getRoleIcon()}
                </div>
            </div>
        </div>
    );
}
