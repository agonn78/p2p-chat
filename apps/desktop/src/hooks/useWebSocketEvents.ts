import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../store';
import { shouldPromoteStatus } from '../services/messages/status';
import type { ChannelMessage, Message, MessageReaction, MessageStatus } from '../types';

interface WsEventPayload {
    type: string;
    message?: Message | ChannelMessage;
    message_id?: string;
    room_id?: string;
    channel_id?: string;
    user_id?: string;
    is_typing?: boolean;
    joined?: boolean;
    status?: string;
    reactions?: MessageReaction[];
}

const MESSAGE_STATUSES: MessageStatus[] = ['sending', 'failed', 'sent', 'delivered', 'read'];

const isMessageStatus = (value: string): value is MessageStatus => {
    return MESSAGE_STATUSES.includes(value as MessageStatus);
};

export function useWebSocketEvents() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const user = useAppStore((s) => s.user);
    const activeRoom = useAppStore((s) => s.activeRoom);
    const activeChannel = useAppStore((s) => s.activeChannel);
    const setWsConnected = useAppStore((s) => s.setWsConnected);
    const addMessage = useAppStore((s) => s.addMessage);
    const updateMessage = useAppStore((s) => s.updateMessage);
    const setTyping = useAppStore((s) => s.setTyping);
    const removeMessage = useAppStore((s) => s.removeMessage);
    const clearMessages = useAppStore((s) => s.clearMessages);
    const setChannelTyping = useAppStore((s) => s.setChannelTyping);
    const setVoicePresence = useAppStore((s) => s.setVoicePresence);
    const setChannelMessageReactions = useAppStore((s) => s.setChannelMessageReactions);

    useEffect(() => {
        if (!isAuthenticated) {
            console.log('[App] Not authenticated, skipping WS listener setup');
            return;
        }

        console.log('[App] 🔌 Setting up WebSocket listener...');

        const setupListener = async () => {
            try {
                const unlisten = await listen<string>('ws-message', (event) => {
                    if (!useAppStore.getState().wsConnected) {
                        setWsConnected(true);
                    }

                    console.log('[App] 📨 WS Event received');

                    try {
                        const payload = JSON.parse(event.payload) as WsEventPayload;

                        if (payload.type === 'NEW_MESSAGE') {
                            const message = payload.message as Message | undefined;
                            if (!message) {
                                return;
                            }

                            console.log('[App] ✉️ NEW_MESSAGE via WebSocket');
                            addMessage(message);

                            if (message.room_id && message.id && message.sender_id !== user?.id) {
                                invoke('api_mark_message_delivered', {
                                    roomId: message.room_id,
                                    messageId: message.id,
                                }).catch(() => undefined);

                                if (message.room_id === activeRoom) {
                                    invoke('api_mark_room_read', {
                                        roomId: message.room_id,
                                        uptoMessageId: message.id,
                                    }).catch(() => undefined);
                                }
                            }
                        } else if (payload.type === 'MESSAGE_EDITED') {
                            const message = payload.message as Message | undefined;
                            if (!message) {
                                return;
                            }

                            console.log('[App] ✏️ MESSAGE_EDITED via WebSocket');
                            updateMessage(message);
                        } else if (payload.type === 'MESSAGE_STATUS') {
                            const nextStatus = typeof payload.status === 'string' && isMessageStatus(payload.status)
                                ? payload.status
                                : undefined;

                            useAppStore.setState((state) => ({
                                messages: state.messages.map((message) =>
                                    message.id === payload.message_id && shouldPromoteStatus(message.status, nextStatus)
                                        ? { ...message, status: nextStatus }
                                        : message
                                ),
                            }));

                            if (payload.message_id && nextStatus) {
                                invoke('api_cache_message_status', {
                                    messageId: payload.message_id,
                                    status: nextStatus,
                                }).catch(() => undefined);
                            }
                        } else if (payload.type === 'TYPING') {
                            if (payload.room_id && payload.user_id) {
                                setTyping(payload.room_id, payload.user_id, !!payload.is_typing);
                            }
                        } else if (payload.type === 'MESSAGE_DELETED') {
                            if (!payload.message_id) {
                                return;
                            }
                            console.log('[App] 🗑️ MESSAGE_DELETED via WebSocket');
                            removeMessage(payload.message_id);
                        } else if (payload.type === 'ALL_MESSAGES_DELETED') {
                            console.log('[App] 🗑️ ALL_MESSAGES_DELETED via WebSocket');
                            if (payload.room_id === activeRoom) {
                                clearMessages();
                            }
                        } else if (payload.type === 'NEW_CHANNEL_MESSAGE') {
                            const message = payload.message as ChannelMessage | undefined;
                            if (!message || payload.channel_id !== activeChannel) {
                                return;
                            }

                            useAppStore.setState((state) => {
                                if (state.channelMessages.some((existing) =>
                                    existing.id === message.id
                                    || (!!existing.client_id && !!message.client_id && existing.client_id === message.client_id)
                                )) {
                                    return state;
                                }

                                return {
                                    channelMessages: [...state.channelMessages, message],
                                };
                            });
                        } else if (payload.type === 'CHANNEL_TYPING') {
                            if (payload.channel_id && payload.user_id) {
                                setChannelTyping(payload.channel_id, payload.user_id, !!payload.is_typing);
                            }
                        } else if (payload.type === 'VOICE_PRESENCE') {
                            if (payload.channel_id && payload.user_id) {
                                setVoicePresence(payload.channel_id, payload.user_id, !!payload.joined);
                                if (payload.user_id === user?.id && !payload.joined) {
                                    useAppStore.setState({ activeVoiceChannel: null });
                                }
                            }
                        } else if (payload.type === 'CHANNEL_MESSAGE_EDITED') {
                            const message = payload.message as ChannelMessage | undefined;
                            if (!message) {
                                return;
                            }

                            useAppStore.setState((state) => ({
                                channelMessages: state.channelMessages.map((existing) =>
                                    existing.id === message.id ? { ...existing, ...message } : existing
                                ),
                            }));
                        } else if (payload.type === 'CHANNEL_MESSAGE_REACTIONS') {
                            if (payload.message_id) {
                                setChannelMessageReactions(payload.message_id, payload.reactions || []);
                            }
                        } else if (payload.type === 'MENTION_ALERT') {
                            console.log('[App] 🔔 Mention alert:', payload);
                        }
                    } catch (error) {
                        console.error('[App] ❌ Failed to parse WS message:', error);
                    }
                });

                const unlistenStatus = await listen<boolean>('ws-status', (event) => {
                    console.log('[App] 📡 WS Status changed:', event.payload);
                    setWsConnected(event.payload);
                });

                const unlistenReconnected = await listen<boolean>('ws-reconnected', async () => {
                    if (user?.id) {
                        try {
                            await invoke('identify_user', { userId: user.id });
                            console.log('[App] 🔁 Re-identified user after WS reconnect');
                        } catch (error) {
                            console.error('[App] Failed to re-identify after reconnect:', error);
                        }
                    }

                    try {
                        const retried = await invoke<number>('api_drain_outbox', { limit: 200 });
                        if (retried > 0) {
                            console.log(`[App] 🔁 Drained ${retried} queued messages after reconnect`);
                        }
                    } catch (error) {
                        console.warn('[App] Failed to drain outbox after reconnect:', error);
                    }
                });

                console.log('[App] ✅ WebSocket listeners attached');
                setWsConnected(true);

                return () => {
                    unlisten();
                    unlistenStatus();
                    unlistenReconnected();
                };
            } catch (error) {
                console.error('[App] ❌ Failed to setup WS listener:', error);
                setWsConnected(false);
                return () => { };
            }
        };

        let cleanup: (() => void) | null = null;
        setupListener().then((fn) => {
            cleanup = fn;
        });

        return () => {
            console.log('[App] 🔌 Cleaning up WebSocket listener');
            if (cleanup) {
                cleanup();
            }
        };
    }, [
        isAuthenticated,
        activeRoom,
        activeChannel,
        user?.id,
        addMessage,
        updateMessage,
        setTyping,
        removeMessage,
        clearMessages,
        setChannelTyping,
        setVoicePresence,
        setChannelMessageReactions,
        setWsConnected,
    ]);
}
