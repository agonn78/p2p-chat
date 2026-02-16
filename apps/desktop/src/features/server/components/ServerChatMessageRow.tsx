import { SmilePlus } from 'lucide-react';
import type { ChannelMessage, MessageReaction } from '../../../types';

interface ServerChatMessageRowProps {
    message: ChannelMessage;
    userId?: string;
    senderName: string;
    reactions: MessageReaction[];
    onToggleReaction: (emoji: string) => void;
    onQuickReaction: () => void;
}

export function ServerChatMessageRow({
    message,
    userId,
    senderName,
    reactions,
    onToggleReaction,
    onQuickReaction,
}: ServerChatMessageRowProps) {
    return (
        <div className="flex gap-3 hover:bg-white/5 p-2 rounded-lg">
            <div className="w-10 h-10 rounded-full bg-gradient-to-br from-primary to-secondary flex-shrink-0" />
            <div>
                <div className="flex items-center gap-2">
                    <span className="font-semibold text-sm">{senderName}</span>
                    <span className="text-xs text-gray-500">{new Date(message.created_at).toLocaleTimeString()}</span>
                    {message.status && message.sender_id === userId && (
                        <span className="text-xs text-gray-500 capitalize">{message.status}</span>
                    )}
                </div>

                <p className="text-gray-200">{message.content}</p>

                <div className="mt-2 flex items-center flex-wrap gap-1.5">
                    {reactions.map((reaction) => {
                        const mine = reaction.user_ids.includes(userId || '');
                        return (
                            <button
                                key={`${message.id}-${reaction.emoji}`}
                                onClick={() => onToggleReaction(reaction.emoji)}
                                className={`px-2 py-1 rounded-full text-xs border transition ${mine
                                    ? 'bg-primary/30 border-primary/60 text-white'
                                    : 'bg-white/5 border-white/10 text-gray-200 hover:bg-white/10'
                                    }`}
                                title={mine ? 'Remove reaction' : 'Add reaction'}
                            >
                                <span className="mr-1">{reaction.emoji}</span>
                                <span>{reaction.count}</span>
                            </button>
                        );
                    })}

                    {!message.id.startsWith('local-') && (
                        <button
                            onClick={onQuickReaction}
                            className="p-1 rounded-full text-gray-400 hover:text-white hover:bg-white/10 transition"
                            title="React with thumbs up"
                        >
                            <SmilePlus className="w-4 h-4" />
                        </button>
                    )}
                </div>
            </div>
        </div>
    );
}
