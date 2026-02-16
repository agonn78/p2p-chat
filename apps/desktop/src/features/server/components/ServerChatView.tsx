import { useEffect, useRef, useState } from 'react';
import { Hash, Send } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../store';
import type { ChannelMessage, MessageReaction } from '../../../types';
import { ServerChatMessageRow } from './ServerChatMessageRow';

export function ServerChatView() {
    const activeServer = useAppStore((s) => s.activeServer);
    const channels = useAppStore((s) => s.channels);
    const activeChannel = useAppStore((s) => s.activeChannel);
    const user = useAppStore((s) => s.user);
    const serverMembers = useAppStore((s) => s.serverMembers);
    const channelMessages = useAppStore((s) => s.channelMessages);
    const channelReactions = useAppStore((s) => s.channelReactions);
    const hasMoreChannelMessages = useAppStore((s) => s.hasMoreChannelMessages);
    const isLoadingMoreChannelMessages = useAppStore((s) => s.isLoadingMoreChannelMessages);
    const typingByChannel = useAppStore((s) => s.typingByChannel);
    const sendChannelMessage = useAppStore((s) => s.sendChannelMessage);
    const loadOlderChannelMessages = useAppStore((s) => s.loadOlderChannelMessages);
    const fetchChannelMessageReactions = useAppStore((s) => s.fetchChannelMessageReactions);
    const toggleChannelReaction = useAppStore((s) => s.toggleChannelReaction);

    const [input, setInput] = useState('');
    const messagesContainerRef = useRef<HTMLDivElement>(null);
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const typingTimeoutRef = useRef<number | null>(null);
    const channelLoadOlderInFlightRef = useRef(false);
    const lastScrolledChannelRef = useRef<string | null>(null);

    if (!activeServer || !activeChannel) {
        return null;
    }

    const channel = channels.find((c) => c.id === activeChannel);
    const typingUserIds = typingByChannel[activeChannel] || [];
    const typingMemberName = typingUserIds
        .filter((id) => id !== user?.id)
        .map((id) => serverMembers.find((m) => m.user_id === id)?.username || 'Someone')[0];

    useEffect(() => {
        const isTyping = input.trim().length > 0;

        invoke('api_send_channel_typing', {
            serverId: activeServer,
            channelId: activeChannel,
            isTyping,
        }).catch(() => undefined);

        if (typingTimeoutRef.current) {
            window.clearTimeout(typingTimeoutRef.current);
        }

        if (isTyping) {
            typingTimeoutRef.current = window.setTimeout(() => {
                invoke('api_send_channel_typing', {
                    serverId: activeServer,
                    channelId: activeChannel,
                    isTyping: false,
                }).catch(() => undefined);
            }, 2500);
        }

        return () => {
            if (typingTimeoutRef.current) {
                window.clearTimeout(typingTimeoutRef.current);
                typingTimeoutRef.current = null;
            }
        };
    }, [input, activeServer, activeChannel]);

    useEffect(() => {
        for (const msg of channelMessages) {
            if (msg.id.startsWith('local-')) {
                continue;
            }
            const hasCached = !!channelReactions[msg.id];
            if (!hasCached) {
                void fetchChannelMessageReactions(activeServer, activeChannel, msg.id);
            }
        }
    }, [channelMessages, channelReactions, activeServer, activeChannel, fetchChannelMessageReactions]);

    useEffect(() => {
        const container = messagesContainerRef.current;
        if (!container) {
            return;
        }

        if (lastScrolledChannelRef.current !== activeChannel) {
            if (channelMessages.length > 0) {
                messagesEndRef.current?.scrollIntoView({ behavior: 'auto' });
                lastScrolledChannelRef.current = activeChannel;
            }
            return;
        }

        const distanceToBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
        if (distanceToBottom < 120) {
            messagesEndRef.current?.scrollIntoView({ behavior: 'auto' });
        }
    }, [channelMessages.length, activeChannel]);

    const handleSend = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!input.trim()) {
            return;
        }
        await sendChannelMessage(activeServer, activeChannel, input);
        setInput('');
    };

    const renderMessageSender = (message: ChannelMessage): string => {
        if (message.sender_id === user?.id) {
            return user?.username || 'Me';
        }

        return message.sender_username || serverMembers.find((m) => m.user_id === message.sender_id)?.username || 'Member';
    };

    const renderMessageReactions = (message: ChannelMessage): MessageReaction[] => {
        return channelReactions[message.id] || message.reactions || [];
    };

    return (
        <>
            <div className="h-14 px-4 flex items-center border-b border-white/5">
                <Hash className="w-5 h-5 text-gray-400 mr-2" />
                <h2 className="font-bold">{channel?.name || 'Channel'}</h2>
            </div>

            <div
                ref={messagesContainerRef}
                className="flex-1 overflow-y-auto p-4 space-y-4"
                onScroll={(e) => {
                    const el = e.currentTarget;
                    if (
                        el.scrollTop < 80
                        && hasMoreChannelMessages
                        && !isLoadingMoreChannelMessages
                        && !channelLoadOlderInFlightRef.current
                    ) {
                        const previousHeight = el.scrollHeight;
                        const previousTop = el.scrollTop;
                        channelLoadOlderInFlightRef.current = true;

                        void loadOlderChannelMessages().finally(() => {
                            window.requestAnimationFrame(() => {
                                const updated = messagesContainerRef.current;
                                if (updated) {
                                    const delta = updated.scrollHeight - previousHeight;
                                    updated.scrollTop = previousTop + Math.max(0, delta);
                                }
                                channelLoadOlderInFlightRef.current = false;
                            });
                        });
                    }
                }}
            >
                {(hasMoreChannelMessages || isLoadingMoreChannelMessages) && (
                    <div className="flex justify-center mb-2">
                        <button
                            onClick={() => {
                                const container = messagesContainerRef.current;
                                if (
                                    !container
                                    || isLoadingMoreChannelMessages
                                    || !hasMoreChannelMessages
                                    || channelLoadOlderInFlightRef.current
                                ) {
                                    return;
                                }

                                const previousHeight = container.scrollHeight;
                                const previousTop = container.scrollTop;
                                channelLoadOlderInFlightRef.current = true;

                                void loadOlderChannelMessages().finally(() => {
                                    window.requestAnimationFrame(() => {
                                        const updated = messagesContainerRef.current;
                                        if (updated) {
                                            const delta = updated.scrollHeight - previousHeight;
                                            updated.scrollTop = previousTop + Math.max(0, delta);
                                        }
                                        channelLoadOlderInFlightRef.current = false;
                                    });
                                });
                            }}
                            disabled={isLoadingMoreChannelMessages}
                            className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10 disabled:opacity-60"
                        >
                            {isLoadingMoreChannelMessages ? 'Loading older messages...' : 'Load older messages'}
                        </button>
                    </div>
                )}

                {channelMessages.map((message) => (
                    <ServerChatMessageRow
                        key={message.id}
                        message={message}
                        userId={user?.id}
                        senderName={renderMessageSender(message)}
                        reactions={renderMessageReactions(message)}
                        onToggleReaction={(emoji) => toggleChannelReaction(activeServer, activeChannel, message.id, emoji)}
                        onQuickReaction={() => toggleChannelReaction(activeServer, activeChannel, message.id, '👍')}
                    />
                ))}

                <div ref={messagesEndRef} />
            </div>

            {typingMemberName && (
                <div className="px-4 pb-1 text-xs text-gray-400">{typingMemberName} is typing...</div>
            )}

            <form onSubmit={handleSend} className="p-4 border-t border-white/5">
                <div className="flex items-center gap-2 bg-white/5 rounded-lg px-4 py-2">
                    <input
                        type="text"
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        placeholder={`Message #${channel?.name || 'channel'}`}
                        className="flex-1 bg-transparent outline-none"
                    />
                    <button type="submit" className="p-2 hover:bg-white/10 rounded-lg">
                        <Send className="w-5 h-5" />
                    </button>
                </div>
            </form>
        </>
    );
}
