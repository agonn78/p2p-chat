import { useState, useEffect } from 'react';
import { Phone, PhoneOff, Mic, MicOff, X } from 'lucide-react';
import { useAppStore } from '../store';

export function CallOverlay() {
    const activeCall = useAppStore((s) => s.activeCall);
    const endCall = useAppStore((s) => s.endCall);
    const cancelOutgoingCall = useAppStore((s) => s.cancelOutgoingCall);

    const [isMuted, setIsMuted] = useState(false);
    const [callDuration, setCallDuration] = useState(0);

    // Timer for call duration
    useEffect(() => {
        if (activeCall?.status === 'connected' && activeCall.startTime) {
            const interval = setInterval(() => {
                setCallDuration(Math.floor((Date.now() - activeCall.startTime!) / 1000));
            }, 1000);
            return () => clearInterval(interval);
        }
        setCallDuration(0);
    }, [activeCall?.status, activeCall?.startTime]);

    // Only show for calling, connecting, or connected states
    if (!activeCall || activeCall.status === 'idle' || activeCall.status === 'ended' || activeCall.status === 'ringing') {
        return null;
    }

    const formatDuration = (seconds: number) => {
        const mins = Math.floor(seconds / 60);
        const secs = seconds % 60;
        return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    };

    const handleEndCall = () => {
        if (activeCall.status === 'calling') {
            cancelOutgoingCall();
        } else {
            endCall();
        }
    };

    const toggleMute = () => {
        setIsMuted(!isMuted);
        // TODO: Actually mute the audio stream
    };

    return (
        <div className="fixed top-4 right-4 z-50 w-72 bg-surface/95 backdrop-blur-lg rounded-xl border border-white/10 shadow-2xl overflow-hidden">
            {/* Header */}
            <div className="p-4 bg-gradient-to-r from-primary/20 to-secondary/20 border-b border-white/5">
                <div className="flex items-center gap-3">
                    {/* Avatar */}
                    <div className="w-12 h-12 rounded-full bg-gradient-to-br from-primary to-secondary flex items-center justify-center flex-shrink-0">
                        <span className="text-lg font-bold">
                            {activeCall.peerName?.charAt(0).toUpperCase()}
                        </span>
                    </div>

                    <div className="flex-1 min-w-0">
                        <h3 className="font-semibold truncate">{activeCall.peerName}</h3>
                        <div className="flex items-center gap-2 text-sm">
                            {activeCall.status === 'connected' ? (
                                <>
                                    <span className="text-green-400">● Connected</span>
                                    <span className="text-gray-400">{formatDuration(callDuration)}</span>
                                </>
                            ) : activeCall.status === 'calling' ? (
                                <span className="text-yellow-400 animate-pulse">Calling...</span>
                            ) : (
                                <span className="text-yellow-400 animate-pulse">Connecting...</span>
                            )}
                        </div>
                    </div>
                </div>
            </div>

            {/* Controls */}
            <div className="p-4 flex justify-center gap-4">
                {/* Mute button */}
                <button
                    onClick={toggleMute}
                    className={`w-12 h-12 rounded-full flex items-center justify-center transition ${isMuted
                            ? 'bg-red-500/20 text-red-400 hover:bg-red-500/30'
                            : 'bg-white/10 hover:bg-white/20'
                        }`}
                    title={isMuted ? 'Unmute' : 'Mute'}
                >
                    {isMuted ? <MicOff className="w-5 h-5" /> : <Mic className="w-5 h-5" />}
                </button>

                {/* End Call button */}
                <button
                    onClick={handleEndCall}
                    className="w-12 h-12 rounded-full bg-red-500 hover:bg-red-600 flex items-center justify-center transition shadow-lg"
                    title="End Call"
                >
                    <PhoneOff className="w-5 h-5" />
                </button>
            </div>

            {/* E2EE indicator */}
            <div className="px-4 pb-3 text-center">
                <span className="text-xs text-gray-500">🔒 End-to-end encrypted</span>
            </div>
        </div>
    );
}
