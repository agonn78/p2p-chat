import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../store';

export function useActiveRoomReadReceipt() {
    const user = useAppStore((s) => s.user);
    const activeRoom = useAppStore((s) => s.activeRoom);
    const messages = useAppStore((s) => s.messages);

    useEffect(() => {
        if (!activeRoom || messages.length === 0) {
            return;
        }

        const latest = messages[messages.length - 1];
        if (!latest?.id || latest.sender_id === user?.id) {
            return;
        }

        invoke('api_mark_room_read', {
            roomId: activeRoom,
            uptoMessageId: latest.id,
        }).catch(() => undefined);
    }, [activeRoom, messages.length, user?.id]);
}
