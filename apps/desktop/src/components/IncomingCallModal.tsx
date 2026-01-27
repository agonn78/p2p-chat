import { Phone, PhoneOff } from 'lucide-react';
import { useAppStore } from '../store';

export function IncomingCallModal() {
    const activeCall = useAppStore((s) => s.activeCall);
    const acceptIncomingCall = useAppStore((s) => s.acceptIncomingCall);
    const declineIncomingCall = useAppStore((s) => s.declineIncomingCall);

    // Only show for ringing state
    if (!activeCall || activeCall.status !== 'ringing') return null;

    return (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-md flex items-center justify-center z-50">
            <div className="bg-surface rounded-2xl p-8 w-80 text-center border border-white/10 shadow-2xl">
                {/* Caller Avatar with pulse animation */}
                <div className="relative mb-6">
                    <div className="w-24 h-24 mx-auto rounded-full bg-gradient-to-br from-primary to-secondary flex items-center justify-center">
                        <span className="text-3xl font-bold">
                            {activeCall.peerName?.charAt(0).toUpperCase()}
                        </span>
                    </div>
                    {/* Pulse rings */}
                    <div className="absolute inset-0 w-24 h-24 mx-auto rounded-full bg-primary/20 animate-ping" />
                </div>

                {/* Caller name */}
                <h2 className="text-xl font-bold mb-2">{activeCall.peerName}</h2>
                <p className="text-gray-400 mb-8">Incoming voice call...</p>

                {/* Action buttons */}
                <div className="flex justify-center gap-6">
                    <button
                        onClick={declineIncomingCall}
                        className="w-16 h-16 rounded-full bg-red-500 hover:bg-red-600 flex items-center justify-center transition-transform hover:scale-110 shadow-lg"
                        title="Decline"
                    >
                        <PhoneOff className="w-7 h-7" />
                    </button>
                    <button
                        onClick={acceptIncomingCall}
                        className="w-16 h-16 rounded-full bg-green-500 hover:bg-green-600 flex items-center justify-center transition-transform hover:scale-110 shadow-lg"
                        title="Accept"
                    >
                        <Phone className="w-7 h-7" />
                    </button>
                </div>
            </div>
        </div>
    );
}
