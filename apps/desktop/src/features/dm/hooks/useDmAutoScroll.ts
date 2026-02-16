import { useEffect } from 'react';
import type { MutableRefObject, RefObject } from 'react';

interface UseDmAutoScrollParams {
    activeRoom: string | null;
    messageCount: number;
    containerRef: RefObject<HTMLDivElement>;
    endRef: RefObject<HTMLDivElement>;
    lastScrolledRoomRef: MutableRefObject<string | null>;
}

export function useDmAutoScroll({
    activeRoom,
    messageCount,
    containerRef,
    endRef,
    lastScrolledRoomRef,
}: UseDmAutoScrollParams) {
    useEffect(() => {
        const container = containerRef.current;
        if (!container) {
            return;
        }

        if (!activeRoom) {
            lastScrolledRoomRef.current = null;
            return;
        }

        if (lastScrolledRoomRef.current !== activeRoom) {
            if (messageCount > 0) {
                endRef.current?.scrollIntoView({ behavior: 'auto' });
                lastScrolledRoomRef.current = activeRoom;
            }
            return;
        }

        const distanceToBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
        if (distanceToBottom < 120) {
            endRef.current?.scrollIntoView({ behavior: 'auto' });
        }
    }, [messageCount, activeRoom, containerRef, endRef, lastScrolledRoomRef]);
}
