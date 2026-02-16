import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../store';
import type { CallAcceptedPayload, CallUnavailablePayload, IncomingCallPayload } from '../types';

interface WebRtcOfferPayload {
    peerId: string;
    sdp: string;
}

interface WebRtcAnswerPayload {
    sdp: string;
}

type WebRtcCandidatePayload = Record<string, unknown>;

export function useCallEvents() {
    const isAuthenticated = useAppStore((s) => s.isAuthenticated);
    const handleIncomingCall = useAppStore((s) => s.handleIncomingCall);
    const endCall = useAppStore((s) => s.endCall);

    useEffect(() => {
        if (!isAuthenticated) {
            return;
        }

        const setupCallListeners = async () => {
            const unlistenIncoming = await listen<IncomingCallPayload>('incoming-call', (event) => {
                console.log('[App] 📞 Incoming call event:', event.payload);
                handleIncomingCall(event.payload);
            });

            const unlistenAccepted = await listen<CallAcceptedPayload>('call-accepted', async (event) => {
                console.log('[CALL-DEBUG] ===== CALL ACCEPTED EVENT =====');
                console.log('[CALL-DEBUG] Payload:', JSON.stringify(event.payload, null, 2));

                try {
                    await invoke('complete_call_handshake', { peerPublicKey: event.payload.publicKey });
                    console.log('[CALL-DEBUG] ✅ complete_call_handshake succeeded');

                    console.log('[CALL-DEBUG] Initializing WebRTC audio call...');
                    await invoke('init_audio_call', { targetId: event.payload.peerId });
                    console.log('[CALL-DEBUG] ✅ WebRTC audio call initialized');

                    useAppStore.setState((state) => ({
                        activeCall: state.activeCall
                            ? {
                                ...state.activeCall,
                                status: 'connected',
                                startTime: Date.now(),
                            }
                            : null,
                    }));
                } catch (error) {
                    console.error('[CALL-DEBUG] ❌ E2EE handshake failed:', error);
                    await endCall();
                }
            });

            const unlistenOffer = await listen<WebRtcOfferPayload>('webrtc-offer', async (event) => {
                console.log('[WEBRTC] Received Offer, handling...');
                try {
                    await invoke('handle_audio_offer', {
                        targetId: event.payload.peerId,
                        sdp: event.payload.sdp,
                    });
                    useAppStore.setState((state) => ({
                        activeCall: state.activeCall
                            ? {
                                ...state.activeCall,
                                status: 'connected',
                                startTime: state.activeCall.startTime ?? Date.now(),
                            }
                            : null,
                    }));
                } catch (error) {
                    console.error('[WEBRTC] Failed to handle offer:', error);
                    await endCall();
                }
            });

            const unlistenAnswer = await listen<WebRtcAnswerPayload>('webrtc-answer', async (event) => {
                console.log('[WEBRTC] Received Answer, handling...');
                try {
                    await invoke('handle_audio_answer', { sdp: event.payload.sdp });
                } catch (error) {
                    console.error('[WEBRTC] Failed to handle answer:', error);
                }
            });

            const unlistenCandidate = await listen<WebRtcCandidatePayload>('webrtc-candidate', async (event) => {
                try {
                    await invoke('handle_ice_candidate', { payload: event.payload });
                } catch (error) {
                    console.error('[WEBRTC] Failed to handle candidate:', error);
                }
            });

            const resetActiveCall = () => {
                useAppStore.setState({ activeCall: null });
                invoke('reset_call_media').catch(() => undefined);
            };

            const unlistenDeclined = await listen<string>('call-declined', () => {
                console.log('[CALL-DEBUG] ===== CALL DECLINED EVENT =====');
                resetActiveCall();
            });

            const unlistenEnded = await listen<string>('call-ended', () => {
                console.log('[CALL-DEBUG] ===== CALL ENDED EVENT =====');
                resetActiveCall();
            });

            const unlistenBusy = await listen<string>('call-busy', () => {
                console.log('[App] 📳 Target is busy');
                resetActiveCall();
            });

            const unlistenCancelled = await listen<string>('call-cancelled', () => {
                console.log('[App] 🚫 Call cancelled by caller');
                resetActiveCall();
            });

            const unlistenUnavailable = await listen<CallUnavailablePayload>('call-unavailable', (event) => {
                console.warn(`[App] 🚫 Call unavailable (${event.payload.reason}) for ${event.payload.targetId}`);
                resetActiveCall();
            });

            return () => {
                unlistenIncoming();
                unlistenAccepted();
                unlistenOffer();
                unlistenAnswer();
                unlistenCandidate();
                unlistenDeclined();
                unlistenEnded();
                unlistenBusy();
                unlistenCancelled();
                unlistenUnavailable();
            };
        };

        let cleanup: (() => void) | null = null;
        setupCallListeners().then((fn) => {
            cleanup = fn;
        });

        return () => {
            if (cleanup) {
                cleanup();
            }
        };
    }, [isAuthenticated, handleIncomingCall, endCall]);
}
