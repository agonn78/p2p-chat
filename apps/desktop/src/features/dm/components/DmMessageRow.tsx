import { Check, Copy, Pencil, Trash2, X } from 'lucide-react';
import { MessageContent } from '../../../components/MessageContent';
import type { Message } from '../../../types';

interface DmMessageRowProps {
    message: Message;
    senderName: string;
    isOwn: boolean;
    isHovered: boolean;
    isEditing: boolean;
    displayContent: string;
    editInput: string;
    onHoverMessage: (messageId: string | null) => void;
    onCopyMessage: (content: string) => void;
    onStartEditingMessage: (messageId: string, content: string) => void;
    onDeleteMessage: (messageId: string) => Promise<void>;
    onEditInputChange: (value: string) => void;
    onSaveEditedMessage: () => Promise<void>;
    onCancelEditingMessage: () => void;
}

export function DmMessageRow({
    message,
    senderName,
    isOwn,
    isHovered,
    isEditing,
    displayContent,
    editInput,
    onHoverMessage,
    onCopyMessage,
    onStartEditingMessage,
    onDeleteMessage,
    onEditInputChange,
    onSaveEditedMessage,
    onCancelEditingMessage,
}: DmMessageRowProps) {
    return (
        <div
            className="flex group hover:bg-white/[0.02] -mx-4 px-4 py-1.5 transition relative"
            onMouseEnter={() => onHoverMessage(message.id)}
            onMouseLeave={() => onHoverMessage(null)}
        >
            <div className="w-10 h-10 rounded-full bg-gray-700 mt-1 flex-shrink-0" />

            <div className="ml-3 flex-1 min-w-0">
                <div className="flex items-baseline gap-2">
                    <span className={`font-medium ${isOwn ? 'text-primary' : 'text-gray-200'}`}>
                        {senderName}
                    </span>
                    <span className="text-xs text-gray-500">
                        {new Date(message.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                    </span>
                    {isOwn && message.status && (
                        <span className="text-xs text-gray-500 capitalize">{message.status}</span>
                    )}
                    {message.nonce && (
                        <span className="text-xs text-green-500" title="End-to-end encrypted">
                            🔒
                        </span>
                    )}
                </div>

                {isEditing ? (
                    <div className="mt-1 flex items-center gap-2">
                        <input
                            value={editInput}
                            onChange={(e) => onEditInputChange(e.target.value)}
                            className="flex-1 bg-black/30 border border-white/10 rounded px-2 py-1 text-sm outline-none focus:border-primary/50"
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') {
                                    void onSaveEditedMessage();
                                }
                                if (e.key === 'Escape') {
                                    onCancelEditingMessage();
                                }
                            }}
                            autoFocus
                        />
                        <button
                            onClick={() => void onSaveEditedMessage()}
                            className="p-1.5 hover:bg-green-500/20 rounded text-green-400 transition"
                            title="Save edit"
                        >
                            <Check className="w-4 h-4" />
                        </button>
                        <button
                            onClick={onCancelEditingMessage}
                            className="p-1.5 hover:bg-red-500/20 rounded text-red-400 transition"
                            title="Cancel edit"
                        >
                            <X className="w-4 h-4" />
                        </button>
                    </div>
                ) : (
                    <div className="text-gray-300">
                        <MessageContent content={displayContent} isEncrypted={!!message.nonce} />
                    </div>
                )}
            </div>

            {isHovered && (
                <div className="absolute right-4 top-1/2 -translate-y-1/2 flex items-center gap-1 bg-surface/90 backdrop-blur-sm rounded-lg p-1 border border-white/10">
                    <button
                        onClick={() => onCopyMessage(displayContent)}
                        className="p-1.5 hover:bg-white/10 rounded text-gray-400 hover:text-white transition"
                        title="Copy message"
                    >
                        <Copy className="w-4 h-4" />
                    </button>
                    {isOwn && (
                        <button
                            onClick={() => onStartEditingMessage(message.id, displayContent)}
                            className="p-1.5 hover:bg-white/10 rounded text-gray-400 hover:text-white transition"
                            title="Edit message"
                        >
                            <Pencil className="w-4 h-4" />
                        </button>
                    )}
                    {isOwn && (
                        <button
                            onClick={() => void onDeleteMessage(message.id)}
                            className="p-1.5 hover:bg-red-500/20 rounded text-gray-400 hover:text-red-400 transition"
                            title="Delete message"
                        >
                            <Trash2 className="w-4 h-4" />
                        </button>
                    )}
                </div>
            )}
        </div>
    );
}
