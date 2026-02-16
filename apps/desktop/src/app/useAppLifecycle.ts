import { useActiveRoomReadReceipt } from '../hooks/useActiveRoomReadReceipt';
import { useAppStartup } from '../hooks/useAppStartup';
import { useCallEvents } from '../hooks/useCallEvents';
import { useOutgoingCallTimeout } from '../hooks/useOutgoingCallTimeout';
import { usePollingFallback } from '../hooks/usePollingFallback';
import { useWebSocketEvents } from '../hooks/useWebSocketEvents';

export function useAppLifecycle() {
    useAppStartup();
    useWebSocketEvents();
    useCallEvents();
    useOutgoingCallTimeout();
    usePollingFallback();
    useActiveRoomReadReceipt();
}
