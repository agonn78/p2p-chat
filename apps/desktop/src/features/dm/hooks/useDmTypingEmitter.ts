import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { MutableRefObject } from 'react';

interface UseDmTypingEmitterParams {
    isAuthenticated: boolean;
    activeRoom: string | null;
    msgInput: string;
    typingTimeoutRef: MutableRefObject<number | null>;
}

export function useDmTypingEmitter({
    isAuthenticated,
    activeRoom,
    msgInput,
    typingTimeoutRef,
}: UseDmTypingEmitterParams) {
    useEffect(() => {
        if (!isAuthenticated) {
            return;
        }

        const isTyping = msgInput.trim().length > 0;

        if (activeRoom) {
            invoke('api_send_typing', { roomId: activeRoom, isTyping }).catch(() => undefined);
            if (typingTimeoutRef.current) {
                window.clearTimeout(typingTimeoutRef.current);
            }

            if (isTyping) {
                typingTimeoutRef.current = window.setTimeout(() => {
                    invoke('api_send_typing', { roomId: activeRoom, isTyping: false }).catch(() => undefined);
                }, 2500);
            }
        }

        return () => {
            if (typingTimeoutRef.current) {
                window.clearTimeout(typingTimeoutRef.current);
                typingTimeoutRef.current = null;
            }
        };
    }, [msgInput, activeRoom, isAuthenticated, typingTimeoutRef]);
}
