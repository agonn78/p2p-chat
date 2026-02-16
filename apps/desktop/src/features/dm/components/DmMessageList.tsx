import type { MutableRefObject, RefObject } from 'react';
import { Hash } from 'lucide-react';
import { DmMessageRow } from './DmMessageRow';
import type { Message } from '../../../types';

interface DmMessageListProps {
    messages: Message[];
    userId?: string;
    activeFriendName: string;
    hoveredMessageId: string | null;
    editingMessageId: string | null;
    editInput: string;
    hasMoreMessages: boolean;
    isLoadingMoreMessages: boolean;
    dmLoadOlderInFlightRef: MutableRefObject<boolean>;
    dmMessagesContainerRef: RefObject<HTMLDivElement>;
    messagesEndRef: RefObject<HTMLDivElement>;
    getSenderName: (senderId: string | undefined) => string;
    decryptMessageContent: (message: Message) => string;
    loadOlderMessages: () => Promise<void>;
    onHoverMessage: (messageId: string | null) => void;
    onCopyMessage: (content: string) => void;
    onDeleteMessage: (messageId: string) => Promise<void>;
    onStartEditingMessage: (messageId: string, content: string) => void;
    onEditInputChange: (value: string) => void;
    onSaveEditedMessage: () => Promise<void>;
    onCancelEditingMessage: () => void;
}

const restoreScrollAfterLoad = (
    containerRef: RefObject<HTMLDivElement>,
    previousHeight: number,
    previousTop: number,
    inFlightRef: MutableRefObject<boolean>
) => {
    window.requestAnimationFrame(() => {
        const updated = containerRef.current;
        if (updated) {
            const delta = updated.scrollHeight - previousHeight;
            updated.scrollTop = previousTop + Math.max(0, delta);
        }
        inFlightRef.current = false;
    });
};

export function DmMessageList({
    messages,
    userId,
    activeFriendName,
    hoveredMessageId,
    editingMessageId,
    editInput,
    hasMoreMessages,
    isLoadingMoreMessages,
    dmLoadOlderInFlightRef,
    dmMessagesContainerRef,
    messagesEndRef,
    getSenderName,
    decryptMessageContent,
    loadOlderMessages,
    onHoverMessage,
    onCopyMessage,
    onDeleteMessage,
    onStartEditingMessage,
    onEditInputChange,
    onSaveEditedMessage,
    onCancelEditingMessage,
}: DmMessageListProps) {
    const handleLoadOlderMessages = () => {
        const container = dmMessagesContainerRef.current;
        if (
            !container
            || isLoadingMoreMessages
            || !hasMoreMessages
            || dmLoadOlderInFlightRef.current
        ) {
            return;
        }

        const previousHeight = container.scrollHeight;
        const previousTop = container.scrollTop;
        dmLoadOlderInFlightRef.current = true;

        void loadOlderMessages().finally(() => {
            restoreScrollAfterLoad(dmMessagesContainerRef, previousHeight, previousTop, dmLoadOlderInFlightRef);
        });
    };

    return (
        <div
            ref={dmMessagesContainerRef}
            className="flex-1 overflow-y-auto p-4 space-y-2"
            onScroll={(e) => {
                const el = e.currentTarget;
                if (
                    el.scrollTop < 80
                    && hasMoreMessages
                    && !isLoadingMoreMessages
                    && !dmLoadOlderInFlightRef.current
                ) {
                    const previousHeight = el.scrollHeight;
                    const previousTop = el.scrollTop;
                    dmLoadOlderInFlightRef.current = true;

                    void loadOlderMessages().finally(() => {
                        restoreScrollAfterLoad(dmMessagesContainerRef, previousHeight, previousTop, dmLoadOlderInFlightRef);
                    });
                }
            }}
        >
            {(hasMoreMessages || isLoadingMoreMessages) && (
                <div className="flex justify-center mb-3">
                    <button
                        onClick={handleLoadOlderMessages}
                        disabled={isLoadingMoreMessages}
                        className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10 disabled:opacity-60"
                    >
                        {isLoadingMoreMessages ? 'Loading older messages...' : 'Load older messages'}
                    </button>
                </div>
            )}

            {messages.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-full text-gray-500">
                    <div className="w-24 h-24 rounded-full bg-gradient-to-br from-primary/20 to-secondary/20 flex items-center justify-center mb-4">
                        <Hash className="w-12 h-12 text-primary/50" />
                    </div>
                    <p className="text-lg font-medium">Start the conversation!</p>
                    <p className="text-sm">Send a message to {activeFriendName}</p>
                    <p className="text-xs text-gray-600 mt-4">💡 Try /shrug, /tableflip, or **bold text**</p>
                </div>
            ) : (
                <>
                    {messages.map((message) => {
                        const displayContent = message._decryptedContent || decryptMessageContent(message);
                        const isOwn = message.sender_id === userId;
                        const isHovered = hoveredMessageId === message.id;
                        const isEditing = editingMessageId === message.id;

                        return (
                            <DmMessageRow
                                key={message.id}
                                message={message}
                                senderName={getSenderName(message.sender_id)}
                                isOwn={isOwn}
                                isHovered={isHovered}
                                isEditing={isEditing}
                                displayContent={displayContent}
                                editInput={editInput}
                                onHoverMessage={onHoverMessage}
                                onCopyMessage={onCopyMessage}
                                onStartEditingMessage={onStartEditingMessage}
                                onDeleteMessage={onDeleteMessage}
                                onEditInputChange={onEditInputChange}
                                onSaveEditedMessage={onSaveEditedMessage}
                                onCancelEditingMessage={onCancelEditingMessage}
                            />
                        );
                    })}
                    <div ref={messagesEndRef} />
                </>
            )}
        </div>
    );
}
