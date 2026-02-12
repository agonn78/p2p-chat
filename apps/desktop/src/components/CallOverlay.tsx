import { useState, useEffect } from 'react';
import { PhoneOff, Mic, MicOff, Settings, ChevronDown } from 'lucide-react';
import { useAppStore } from '../store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface AudioDevice {
    id: string;
    name: string;
}

export function CallOverlay() {
    const activeCall = useAppStore((s) => s.activeCall);
    const endCall = useAppStore((s) => s.endCall);
    const cancelOutgoingCall = useAppStore((s) => s.cancelOutgoingCall);

    const [isMuted, setIsMuted] = useState(false);
    const [callDuration, setCallDuration] = useState(0);
    const [showSettings, setShowSettings] = useState(false);
    const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
    const [selectedDevice, setSelectedDevice] = useState<string>('');
    const [isLoadingDevices, setIsLoadingDevices] = useState(false);
    const [isSwitchingDevice, setIsSwitchingDevice] = useState(false);
    const [vuLevel, setVuLevel] = useState(0);

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

    // Listen for VU meter events
    useEffect(() => {
        if (activeCall?.status !== 'connected') return;

        let unlisten: (() => void) | null = null;
        listen<number>('vu-level', (event) => {
            // Convert RMS to a 0-1 scale (clamp)
            const level = Math.min(1, event.payload * 5); // Amplify for visibility
            setVuLevel(level);
        }).then(fn => { unlisten = fn; });

        // Start the VU meter backend
        invoke('start_vu_meter').catch(e => {
            console.warn('[CallOverlay] VU meter not available:', e);
        });

        return () => {
            if (unlisten) unlisten();
            setVuLevel(0);
        };
    }, [activeCall?.status]);

    // Load audio devices when settings opened
    useEffect(() => {
        if (showSettings) {
            loadAudioDevices();
        }
    }, [showSettings]);

    const loadAudioDevices = async () => {
        setIsLoadingDevices(true);
        try {
            const devices = await invoke<AudioDevice[]>('list_audio_devices');
            setAudioDevices(devices);

            const selected = await invoke<AudioDevice | null>('get_selected_audio_device');
            if (selected?.id && devices.some((d) => d.id === selected.id)) {
                setSelectedDevice(selected.id);
            } else {
                const defaultDevice = await invoke<AudioDevice>('get_default_audio_device');
                if (!selectedDevice && devices.length > 0) {
                    setSelectedDevice(defaultDevice?.id || devices[0].id);
                }
            }
        } catch (err) {
            console.error('Failed to load audio devices:', err);
        }
        setIsLoadingDevices(false);
    };

    const switchAudioDevice = async (nextDeviceId: string) => {
        const previous = selectedDevice;
        setSelectedDevice(nextDeviceId);
        setIsSwitchingDevice(true);
        try {
            await invoke('set_audio_device', { deviceId: nextDeviceId });
            console.log('[Audio] Switched device to:', nextDeviceId);
        } catch (err) {
            console.error('[Audio] Failed to switch device:', err);
            setSelectedDevice(previous);
        } finally {
            setIsSwitchingDevice(false);
        }
    };

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
        setShowSettings(false);
    };

    const toggleMute = async () => {
        try {
            const muted = await invoke<boolean>('toggle_mute');
            setIsMuted(muted);
        } catch (e) {
            console.error('[CallOverlay] toggle_mute failed:', e);
            // Fallback to local toggle if command fails
            setIsMuted(prev => !prev);
        }
    };

    // VU meter bar width (percentage)
    const vuWidth = Math.max(2, vuLevel * 100);

    return (
        <div className="fixed top-4 right-4 z-50 w-80 bg-surface/95 backdrop-blur-lg rounded-xl border border-white/10 shadow-2xl overflow-hidden">
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

                    {/* Settings button */}
                    <button
                        onClick={() => setShowSettings(!showSettings)}
                        className={`w-8 h-8 rounded-full flex items-center justify-center transition ${showSettings ? 'bg-white/20' : 'hover:bg-white/10'
                            }`}
                        title="Audio Settings"
                    >
                        <Settings className="w-4 h-4" />
                    </button>
                </div>
            </div>

            {/* VU Meter */}
            {activeCall.status === 'connected' && (
                <div className="px-4 pt-3">
                    <div className="flex items-center gap-2">
                        <Mic className="w-3 h-3 text-gray-400 flex-shrink-0" />
                        <div className="flex-1 h-2 bg-white/5 rounded-full overflow-hidden">
                            <div
                                className="h-full rounded-full transition-all duration-75"
                                style={{
                                    width: `${vuWidth}%`,
                                    background: vuLevel > 0.7
                                        ? 'linear-gradient(90deg, #22c55e, #ef4444)'
                                        : vuLevel > 0.3
                                            ? 'linear-gradient(90deg, #22c55e, #eab308)'
                                            : '#22c55e',
                                }}
                            />
                        </div>
                    </div>
                </div>
            )}

            {/* Audio Settings Panel */}
            {showSettings && (
                <div className="p-3 border-b border-white/5 bg-white/5">
                    <div className="text-xs text-gray-400 mb-2">Microphone</div>
                    <div className="relative">
                        <select
                            value={selectedDevice}
                            onChange={(e) => void switchAudioDevice(e.target.value)}
                            className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm appearance-none cursor-pointer focus:outline-none focus:border-primary/50"
                            disabled={isLoadingDevices || isSwitchingDevice}
                        >
                            {isLoadingDevices || isSwitchingDevice ? (
                                <option>Loading...</option>
                            ) : audioDevices.length === 0 ? (
                                <option>No devices found</option>
                            ) : (
                                audioDevices.map((device) => (
                                    <option key={device.id} value={device.id}>
                                        {device.name}
                                    </option>
                                ))
                            )}
                        </select>
                        <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400 pointer-events-none" />
                    </div>
                    <div className="mt-2 text-xs text-gray-500">
                        {isSwitchingDevice ? 'Switching microphone...' : 'Device changes apply live during call'}
                    </div>
                </div>
            )}

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
