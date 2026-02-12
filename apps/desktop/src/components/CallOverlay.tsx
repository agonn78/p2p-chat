import { useEffect, useMemo, useState } from 'react';
import { PhoneOff, Mic, MicOff, Settings, ChevronDown, Volume2, VolumeX, Ear } from 'lucide-react';
import { useAppStore } from '../store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface AudioDevice {
    id: string;
    name: string;
}

type VoiceMode = 'mute' | 'push_to_talk' | 'voice_activity';
type AudioMode = 'headphones' | 'speakers';

interface AudioSettings {
    mic_gain: number;
    output_volume: number;
    remote_user_volume: number;
    voice_mode: VoiceMode;
    vad_threshold: number;
    noise_suppression: boolean;
    aec: boolean;
    agc: boolean;
    noise_gate: boolean;
    noise_gate_threshold: number;
    limiter: boolean;
    deafen: boolean;
    ptt_key: string;
    audio_mode: AudioMode;
}

const DEFAULT_AUDIO_SETTINGS: AudioSettings = {
    mic_gain: 1,
    output_volume: 1,
    remote_user_volume: 1,
    voice_mode: 'voice_activity',
    vad_threshold: 0.02,
    noise_suppression: true,
    aec: true,
    agc: true,
    noise_gate: true,
    noise_gate_threshold: 0.01,
    limiter: true,
    deafen: false,
    ptt_key: 'V',
    audio_mode: 'headphones',
};

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

export function CallOverlay() {
    const activeCall = useAppStore((s) => s.activeCall);
    const endCall = useAppStore((s) => s.endCall);
    const cancelOutgoingCall = useAppStore((s) => s.cancelOutgoingCall);

    const [isMuted, setIsMuted] = useState(false);
    const [callDuration, setCallDuration] = useState(0);
    const [showSettings, setShowSettings] = useState(false);

    const [inputDevices, setInputDevices] = useState<AudioDevice[]>([]);
    const [outputDevices, setOutputDevices] = useState<AudioDevice[]>([]);
    const [selectedInputDevice, setSelectedInputDevice] = useState('');
    const [selectedOutputDevice, setSelectedOutputDevice] = useState('');
    const [isLoadingDevices, setIsLoadingDevices] = useState(false);
    const [isSwitchingInput, setIsSwitchingInput] = useState(false);
    const [isSwitchingOutput, setIsSwitchingOutput] = useState(false);

    const [settings, setSettings] = useState<AudioSettings>(DEFAULT_AUDIO_SETTINGS);
    const [isSavingSettings, setIsSavingSettings] = useState(false);
    const [vuLevel, setVuLevel] = useState(0);
    const [isPttPressed, setIsPttPressed] = useState(false);

    const remoteName = activeCall?.peerName || 'Remote user';

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
            const level = Math.min(1, event.payload * 6);
            setVuLevel(level);
        }).then((fn) => {
            unlisten = fn;
        });

        invoke('start_vu_meter').catch((e) => {
            console.warn('[CallOverlay] VU meter not available:', e);
        });

        return () => {
            if (unlisten) unlisten();
            setVuLevel(0);
        };
    }, [activeCall?.status]);

    const loadSettings = async () => {
        try {
            const audioSettings = await invoke<AudioSettings>('get_audio_settings');
            setSettings(audioSettings);
        } catch (e) {
            console.warn('[CallOverlay] Using default audio settings:', e);
        }
    };

    const loadDevices = async () => {
        setIsLoadingDevices(true);
        try {
            const [micDevices, speakerDevices] = await Promise.all([
                invoke<AudioDevice[]>('list_audio_devices'),
                invoke<AudioDevice[]>('list_output_devices'),
            ]);
            setInputDevices(micDevices);
            setOutputDevices(speakerDevices);

            const selectedMic = await invoke<AudioDevice | null>('get_selected_audio_device');
            if (selectedMic?.id && micDevices.some((d) => d.id === selectedMic.id)) {
                setSelectedInputDevice(selectedMic.id);
            } else {
                const defaultMic = await invoke<AudioDevice>('get_default_audio_device');
                setSelectedInputDevice(defaultMic?.id || micDevices[0]?.id || '');
            }

            const selectedSpeaker = await invoke<AudioDevice | null>('get_selected_output_device');
            if (selectedSpeaker?.id && speakerDevices.some((d) => d.id === selectedSpeaker.id)) {
                setSelectedOutputDevice(selectedSpeaker.id);
            } else {
                const defaultSpeaker = await invoke<AudioDevice>('get_default_output_device');
                setSelectedOutputDevice(defaultSpeaker?.id || speakerDevices[0]?.id || '');
            }
        } catch (e) {
            console.error('[CallOverlay] Failed to load devices:', e);
        }
        setIsLoadingDevices(false);
    };

    useEffect(() => {
        if (showSettings) {
            void Promise.all([loadDevices(), loadSettings()]);
        }
    }, [showSettings]);

    const saveSettings = async (next: AudioSettings) => {
        setSettings(next);
        setIsSavingSettings(true);
        try {
            await invoke('update_audio_settings', { settings: next });
        } catch (e) {
            console.error('[CallOverlay] Failed to update audio settings:', e);
        } finally {
            setIsSavingSettings(false);
        }
    };

    const updateSetting = <K extends keyof AudioSettings>(key: K, value: AudioSettings[K]) => {
        void saveSettings({ ...settings, [key]: value });
    };

    const switchInputDevice = async (nextDeviceId: string) => {
        const previous = selectedInputDevice;
        setSelectedInputDevice(nextDeviceId);
        setIsSwitchingInput(true);
        try {
            await invoke('set_audio_device', { deviceId: nextDeviceId });
        } catch (e) {
            console.error('[CallOverlay] Failed to switch input device:', e);
            setSelectedInputDevice(previous);
        } finally {
            setIsSwitchingInput(false);
        }
    };

    const switchOutputDevice = async (nextDeviceId: string) => {
        const previous = selectedOutputDevice;
        setSelectedOutputDevice(nextDeviceId);
        setIsSwitchingOutput(true);
        try {
            await invoke('set_output_device', { deviceId: nextDeviceId });
        } catch (e) {
            console.error('[CallOverlay] Failed to switch output device:', e);
            setSelectedOutputDevice(previous);
        } finally {
            setIsSwitchingOutput(false);
        }
    };

    useEffect(() => {
        if (activeCall?.status !== 'connected') return;

        if (settings.voice_mode !== 'push_to_talk') {
            setIsPttPressed(false);
            invoke('set_ptt_active', { active: true }).catch(() => undefined);
            return;
        }

        const expectedKey = settings.ptt_key.trim().toLowerCase() || 'v';
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null;
            if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;

            if (event.key.toLowerCase() === expectedKey) {
                setIsPttPressed((prev) => {
                    if (!prev) {
                        void invoke('set_ptt_active', { active: true });
                    }
                    return true;
                });
            }
        };

        const onKeyUp = (event: KeyboardEvent) => {
            if (event.key.toLowerCase() === expectedKey) {
                setIsPttPressed(false);
                void invoke('set_ptt_active', { active: false });
            }
        };

        window.addEventListener('keydown', onKeyDown);
        window.addEventListener('keyup', onKeyUp);
        void invoke('set_ptt_active', { active: false });

        return () => {
            window.removeEventListener('keydown', onKeyDown);
            window.removeEventListener('keyup', onKeyUp);
            void invoke('set_ptt_active', { active: false });
            setIsPttPressed(false);
        };
    }, [activeCall?.status, settings.voice_mode, settings.ptt_key]);

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
            setIsMuted((prev) => !prev);
        }
    };

    const toggleDeafen = () => {
        void saveSettings({ ...settings, deafen: !settings.deafen });
    };

    const vuWidth = Math.max(2, vuLevel * 100);
    const vadPercent = clamp((settings.vad_threshold / 0.3) * 100, 0, 100);

    const savingLabel = useMemo(() => {
        if (isSwitchingInput) return 'Switching microphone...';
        if (isSwitchingOutput) return 'Switching output device...';
        if (isSavingSettings) return 'Applying audio settings...';
        return 'Changes apply live during call';
    }, [isSwitchingInput, isSwitchingOutput, isSavingSettings]);

    return (
        <div className="fixed top-4 right-4 z-50 w-[420px] max-h-[92vh] bg-surface/95 backdrop-blur-lg rounded-xl border border-white/10 shadow-2xl overflow-hidden flex flex-col">
            <div className="p-4 bg-gradient-to-r from-primary/20 to-secondary/20 border-b border-white/5">
                <div className="flex items-center gap-3">
                    <div className="w-12 h-12 rounded-full bg-gradient-to-br from-primary to-secondary flex items-center justify-center flex-shrink-0">
                        <span className="text-lg font-bold">{activeCall.peerName?.charAt(0).toUpperCase()}</span>
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

                    <button
                        onClick={() => setShowSettings(!showSettings)}
                        className={`w-8 h-8 rounded-full flex items-center justify-center transition ${showSettings ? 'bg-white/20' : 'hover:bg-white/10'}`}
                        title="Audio settings"
                    >
                        <Settings className="w-4 h-4" />
                    </button>
                </div>
            </div>

            {activeCall.status === 'connected' && (
                <div className="px-4 pt-3">
                    <div className="flex items-center gap-2">
                        <Mic className="w-3 h-3 text-gray-400 flex-shrink-0" />
                        <div className="flex-1 h-2 bg-white/5 rounded-full overflow-hidden relative">
                            <div
                                className="h-full rounded-full transition-all duration-75"
                                style={{
                                    width: `${vuWidth}%`,
                                    background:
                                        vuLevel > 0.7
                                            ? 'linear-gradient(90deg, #22c55e, #ef4444)'
                                            : vuLevel > 0.3
                                                ? 'linear-gradient(90deg, #22c55e, #eab308)'
                                                : '#22c55e',
                                }}
                            />
                            {settings.voice_mode === 'voice_activity' && (
                                <div
                                    className="absolute top-0 bottom-0 w-[2px] bg-amber-400"
                                    style={{ left: `${vadPercent}%` }}
                                    title="Voice activation threshold"
                                />
                            )}
                        </div>
                    </div>
                </div>
            )}

            {showSettings && (
                <div className="overflow-y-auto max-h-[62vh] p-3 border-b border-white/5 bg-white/5 space-y-4">
                    <div>
                        <div className="text-xs uppercase tracking-wide text-gray-400 mb-2">Entree (micro)</div>

                        <label className="text-xs text-gray-400">Peripherique d'entree</label>
                        <div className="relative mt-1 mb-3">
                            <select
                                value={selectedInputDevice}
                                onChange={(e) => void switchInputDevice(e.target.value)}
                                className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm appearance-none cursor-pointer focus:outline-none focus:border-primary/50"
                                disabled={isLoadingDevices || isSwitchingInput}
                            >
                                {isLoadingDevices || isSwitchingInput ? (
                                    <option>Loading...</option>
                                ) : inputDevices.length === 0 ? (
                                    <option>No microphone found</option>
                                ) : (
                                    inputDevices.map((device) => (
                                        <option key={device.id} value={device.id}>
                                            {device.name}
                                        </option>
                                    ))
                                )}
                            </select>
                            <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400 pointer-events-none" />
                        </div>

                        <label className="text-xs text-gray-400">Volume micro (gain logiciel): {settings.mic_gain.toFixed(2)}x</label>
                        <input
                            type="range"
                            min={0}
                            max={3}
                            step={0.05}
                            value={settings.mic_gain}
                            onChange={(e) => updateSetting('mic_gain', clamp(Number(e.target.value), 0, 3))}
                            className="w-full accent-primary mt-1 mb-3"
                        />

                        <label className="text-xs text-gray-400">Mode</label>
                        <select
                            value={settings.voice_mode}
                            onChange={(e) => updateSetting('voice_mode', e.target.value as VoiceMode)}
                            className="w-full mt-1 mb-3 px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm focus:outline-none focus:border-primary/50"
                        >
                            <option value="push_to_talk">Push-to-talk</option>
                            <option value="voice_activity">Detection de voix</option>
                            <option value="mute">Mute</option>
                        </select>

                        {settings.voice_mode === 'voice_activity' && (
                            <>
                                <label className="text-xs text-gray-400">
                                    Seuil d'activation: {(settings.vad_threshold * 100).toFixed(1)}%
                                </label>
                                <input
                                    type="range"
                                    min={0}
                                    max={0.3}
                                    step={0.005}
                                    value={settings.vad_threshold}
                                    onChange={(e) => updateSetting('vad_threshold', clamp(Number(e.target.value), 0, 0.3))}
                                    className="w-full accent-amber-400 mt-1 mb-3"
                                />
                            </>
                        )}

                        {settings.voice_mode === 'push_to_talk' && (
                            <div className="mb-3">
                                <label className="text-xs text-gray-400">Raccourci PTT</label>
                                <input
                                    value={settings.ptt_key}
                                    onChange={(e) => {
                                        const value = e.target.value.slice(0, 1).toUpperCase() || 'V';
                                        updateSetting('ptt_key', value);
                                    }}
                                    className="w-full mt-1 px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm focus:outline-none focus:border-primary/50"
                                />
                                <div className="text-[11px] mt-1 text-gray-500">
                                    Etat PTT: {isPttPressed ? 'Transmitting' : 'Idle'}
                                </div>
                            </div>
                        )}

                        <div className="grid grid-cols-2 gap-2 text-xs">
                            <Toggle label="Suppression de bruit" checked={settings.noise_suppression} onToggle={() => updateSetting('noise_suppression', !settings.noise_suppression)} />
                            <Toggle label="Annulation d'echo (AEC)" checked={settings.aec} onToggle={() => updateSetting('aec', !settings.aec)} />
                            <Toggle label="AGC" checked={settings.agc} onToggle={() => updateSetting('agc', !settings.agc)} />
                            <Toggle label="Noise gate" checked={settings.noise_gate} onToggle={() => updateSetting('noise_gate', !settings.noise_gate)} />
                        </div>

                        {settings.noise_gate && (
                            <>
                                <label className="text-xs text-gray-400 mt-3 block">
                                    Seuil noise gate: {(settings.noise_gate_threshold * 100).toFixed(1)}%
                                </label>
                                <input
                                    type="range"
                                    min={0}
                                    max={0.2}
                                    step={0.005}
                                    value={settings.noise_gate_threshold}
                                    onChange={(e) => updateSetting('noise_gate_threshold', clamp(Number(e.target.value), 0, 0.2))}
                                    className="w-full accent-orange-400 mt-1"
                                />
                            </>
                        )}
                    </div>

                    <div>
                        <div className="text-xs uppercase tracking-wide text-gray-400 mb-2">Sortie (casque / haut-parleurs)</div>

                        <label className="text-xs text-gray-400">Peripherique de sortie</label>
                        <div className="relative mt-1 mb-3">
                            <select
                                value={selectedOutputDevice}
                                onChange={(e) => void switchOutputDevice(e.target.value)}
                                className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm appearance-none cursor-pointer focus:outline-none focus:border-primary/50"
                                disabled={isLoadingDevices || isSwitchingOutput}
                            >
                                {isLoadingDevices || isSwitchingOutput ? (
                                    <option>Loading...</option>
                                ) : outputDevices.length === 0 ? (
                                    <option>No output device found</option>
                                ) : (
                                    outputDevices.map((device) => (
                                        <option key={device.id} value={device.id}>
                                            {device.name}
                                        </option>
                                    ))
                                )}
                            </select>
                            <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400 pointer-events-none" />
                        </div>

                        <label className="text-xs text-gray-400">Volume sortie global: {(settings.output_volume * 100).toFixed(0)}%</label>
                        <input
                            type="range"
                            min={0}
                            max={2}
                            step={0.01}
                            value={settings.output_volume}
                            onChange={(e) => updateSetting('output_volume', clamp(Number(e.target.value), 0, 2))}
                            className="w-full accent-primary mt-1 mb-3"
                        />

                        <label className="text-xs text-gray-400">
                            Volume {remoteName}: {(settings.remote_user_volume * 100).toFixed(0)}%
                        </label>
                        <input
                            type="range"
                            min={0}
                            max={2}
                            step={0.01}
                            value={settings.remote_user_volume}
                            onChange={(e) => {
                                const value = clamp(Number(e.target.value), 0, 2);
                                updateSetting('remote_user_volume', value);
                                void invoke('set_remote_user_volume', { volume: value }).catch(() => undefined);
                            }}
                            className="w-full accent-cyan-400 mt-1 mb-3"
                        />

                        <label className="text-xs text-gray-400">Mode audio</label>
                        <select
                            value={settings.audio_mode}
                            onChange={(e) => updateSetting('audio_mode', e.target.value as AudioMode)}
                            className="w-full mt-1 mb-3 px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm focus:outline-none focus:border-primary/50"
                        >
                            <option value="headphones">Casque</option>
                            <option value="speakers">Haut-parleurs</option>
                        </select>

                        <div className="grid grid-cols-2 gap-2 text-xs">
                            <Toggle label="Limiter / protection oreilles" checked={settings.limiter} onToggle={() => updateSetting('limiter', !settings.limiter)} />
                            <Toggle label="Deafen (sortie + micro)" checked={settings.deafen} onToggle={toggleDeafen} />
                        </div>
                    </div>

                    <div className="text-xs text-gray-500">
                        Test micro actif: VU metre en temps reel. {savingLabel}
                    </div>
                </div>
            )}

            <div className="p-4 flex justify-center gap-4">
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

                <button
                    onClick={toggleDeafen}
                    className={`w-12 h-12 rounded-full flex items-center justify-center transition ${settings.deafen
                        ? 'bg-amber-500/20 text-amber-300 hover:bg-amber-500/30'
                        : 'bg-white/10 hover:bg-white/20'
                        }`}
                    title={settings.deafen ? 'Undeafen' : 'Deafen'}
                >
                    {settings.deafen ? <VolumeX className="w-5 h-5" /> : <Ear className="w-5 h-5" />}
                </button>

                <button
                    onClick={handleEndCall}
                    className="w-12 h-12 rounded-full bg-red-500 hover:bg-red-600 flex items-center justify-center transition shadow-lg"
                    title="End call"
                >
                    <PhoneOff className="w-5 h-5" />
                </button>

                <button
                    onClick={() => updateSetting('output_volume', clamp(settings.output_volume + 0.05, 0, 2))}
                    className="w-12 h-12 rounded-full bg-white/10 hover:bg-white/20 flex items-center justify-center transition"
                    title="Increase output"
                >
                    <Volume2 className="w-5 h-5" />
                </button>
            </div>

            <div className="px-4 pb-3 text-center">
                <span className="text-xs text-gray-500">🔒 End-to-end encrypted</span>
            </div>
        </div>
    );
}

function Toggle({ label, checked, onToggle }: { label: string; checked: boolean; onToggle: () => void }) {
    return (
        <button
            onClick={onToggle}
            className={`px-2 py-2 rounded-lg border text-left transition ${checked
                ? 'bg-primary/20 border-primary/40 text-primary'
                : 'bg-white/5 border-white/10 text-gray-300 hover:bg-white/10'
                }`}
        >
            <div className="font-medium">{label}</div>
            <div className="text-[11px] opacity-75">{checked ? 'On' : 'Off'}</div>
        </button>
    );
}
