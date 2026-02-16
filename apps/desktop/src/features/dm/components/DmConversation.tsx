import { useEffect, useRef, useState } from 'react';
import { Hash } from 'lucide-react';
import { useAppStore } from '../../../store';
import { DmMessageList } from './DmMessageList';
import { DmCommandHints } from './DmCommandHints';
import { DmComposer } from './DmComposer';
import { SLASH_COMMANDS, getMatchingSlashCommands } from '../constants/slashCommands';
import { useDmAutoScroll } from '../hooks/useDmAutoScroll';
import { useDmTypingEmitter } from '../hooks/useDmTypingEmitter';
import type { Friend } from '../../../types';

interface DmConversationProps {
    activeFriend: Friend;
}

export function DmConversation({ activeFriend }: DmConversationProps) {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const user = useAppStore((s) => s.user);
    const friends = useAppStore((s) => s.friends);
    const serverMembers = useAppStore((s) => s.serverMembers);

    const activeRoom = useAppStore((s) => s.activeRoom);
    const messages = useAppStore((s) => s.messages);
    const hasMoreMessages = useAppStore((s) => s.hasMoreMessages);
    const isLoadingMoreMessages = useAppStore((s) => s.isLoadingMoreMessages);
    const typingByRoom = useAppStore((s) => s.typingByRoom);
    const loadOlderMessages = useAppStore((s) => s.loadOlderMessages);
    const sendMessage = useAppStore((s) => s.sendMessage);
    const deleteMessage = useAppStore((s) => s.deleteMessage);
    const deleteAllMessages = useAppStore((s) => s.deleteAllMessages);
    const editMessage = useAppStore((s) => s.editMessage);
    const clearMessages = useAppStore((s) => s.clearMessages);
    const decryptMessageContent = useAppStore((s) => s.decryptMessageContent);

    const [msgInput, setMsgInput] = useState('');
    const [showCommandHint, setShowCommandHint] = useState(false);
    const [hoveredMessageId, setHoveredMessageId] = useState<string | null>(null);
    const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
    const [editInput, setEditInput] = useState('');

    const dmMessagesContainerRef = useRef<HTMLDivElement>(null);
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const typingTimeoutRef = useRef<number | null>(null);
    const dmLoadOlderInFlightRef = useRef(false);
    const lastScrolledRoomRef = useRef<string | null>(null);

    useDmAutoScroll({
        activeRoom,
        messageCount: messages.length,
        containerRef: dmMessagesContainerRef,
        endRef: messagesEndRef,
        lastScrolledRoomRef,
    });

    useDmTypingEmitter({
        isAuthenticated,
        activeRoom,
        msgInput,
        typingTimeoutRef,
    });

    useEffect(() => {
        setShowCommandHint(msgInput.startsWith('/') && msgInput.length > 1);
    }, [msgInput]);

    const getSenderName = (senderId: string | undefined) => {
        if (!senderId) {
            return 'Unknown';
        }
        if (senderId === user?.id) {
            return 'Me';
        }
        const friend = friends.find((f) => f.id === senderId);
        if (friend) {
            return friend.username;
        }
        const member = serverMembers.find((m) => m.user_id === senderId);
        return member ? member.username : 'Unknown';
    };

    const handleSendMessage = async () => {
        if (!msgInput.trim() || !activeRoom) {
            return;
        }

        const input = msgInput.trim();

        if (input.startsWith('/')) {
            const command = input.split(' ')[0].toLowerCase();

            if (command === '/clear') {
                clearMessages();
                setMsgInput('');
                return;
            }

            if (command === '/deleteall') {
                if (confirm('Delete ALL messages in this conversation? This cannot be undone.')) {
                    await deleteAllMessages();
                }
                setMsgInput('');
                return;
            }

            const slashCommand = SLASH_COMMANDS[command];
            if (slashCommand?.replacement) {
                await sendMessage(activeRoom, slashCommand.replacement);
                setMsgInput('');
                return;
            }
        }

        try {
            await sendMessage(activeRoom, input);
            setMsgInput('');
        } catch (error) {
            console.error('Failed to send message:', error);
        }
    };

    const handleDeleteMessage = async (messageId: string) => {
        if (confirm('Delete this message?')) {
            await deleteMessage(messageId);
        }
    };

    const handleCopyMessage = (content: string) => {
        navigator.clipboard.writeText(content);
    };

    const handleStartEditingMessage = (messageId: string, content: string) => {
        setEditingMessageId(messageId);
        setEditInput(content);
    };

    const handleCancelEditingMessage = () => {
        setEditingMessageId(null);
        setEditInput('');
    };

    const handleSaveEditedMessage = async () => {
        if (!editingMessageId || !editInput.trim()) {
            return;
        }
        await editMessage(editingMessageId, editInput.trim());
        handleCancelEditingMessage();
    };

    const typingUsers = activeRoom ? typingByRoom[activeRoom] || [] : [];
    const typingMemberName = typingUsers
        .filter((id) => id !== user?.id)
        .map((id) => getSenderName(id))[0] || 'Someone';

    const matchingCommands = getMatchingSlashCommands(msgInput);

    return (
        <>
            <div className="h-14 border-b border-white/5 flex items-center px-4 bg-surface/50 backdrop-blur-sm flex-shrink-0">
                <div className="flex items-center text-gray-200">
                    <Hash className="w-5 h-5 text-gray-400 mr-2" />
                    <span className="font-semibold">{activeFriend.username}</span>
                </div>
            </div>

            <DmMessageList
                messages={messages}
                userId={user?.id}
                activeFriendName={activeFriend.username}
                hoveredMessageId={hoveredMessageId}
                editingMessageId={editingMessageId}
                editInput={editInput}
                hasMoreMessages={hasMoreMessages}
                isLoadingMoreMessages={isLoadingMoreMessages}
                dmLoadOlderInFlightRef={dmLoadOlderInFlightRef}
                dmMessagesContainerRef={dmMessagesContainerRef}
                messagesEndRef={messagesEndRef}
                getSenderName={getSenderName}
                decryptMessageContent={decryptMessageContent}
                loadOlderMessages={loadOlderMessages}
                onHoverMessage={setHoveredMessageId}
                onCopyMessage={handleCopyMessage}
                onDeleteMessage={handleDeleteMessage}
                onStartEditingMessage={handleStartEditingMessage}
                onEditInputChange={setEditInput}
                onSaveEditedMessage={handleSaveEditedMessage}
                onCancelEditingMessage={handleCancelEditingMessage}
            />

            {activeRoom && typingUsers.length > 0 && (
                <div className="px-4 pb-1 text-xs text-gray-400">{typingMemberName} is typing...</div>
            )}

            {showCommandHint && matchingCommands.length > 0 && (
                <DmCommandHints commands={matchingCommands} onSelectCommand={setMsgInput} />
            )}

            <DmComposer
                value={msgInput}
                friendName={activeFriend.username}
                onChange={setMsgInput}
                onSend={handleSendMessage}
            />
        </>
    );
}
