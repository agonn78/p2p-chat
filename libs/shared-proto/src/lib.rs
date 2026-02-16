pub mod signaling {
    use serde::{Deserialize, Serialize};

    pub const PROTOCOL_VERSION: u8 = 1;
    pub const LEGACY_PROTOCOL_VERSION: u8 = 0;

    pub fn is_supported_protocol_version(version: u8) -> bool {
        matches!(version, LEGACY_PROTOCOL_VERSION | PROTOCOL_VERSION)
    }

    fn default_message_version() -> u8 {
        LEGACY_PROTOCOL_VERSION
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Offer {
        pub sdp: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Answer {
        pub sdp: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Candidate {
        pub candidate: String,
        pub sdp_mid: Option<String>,
        pub sdp_m_line_index: Option<u16>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "type", content = "payload")]
    pub enum SignalingMessage {
        #[serde(rename = "offer")]
        Offer {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
            sdp: String,
        },
        #[serde(rename = "answer")]
        Answer {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
            sdp: String,
        },
        #[serde(rename = "candidate")]
        Candidate {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
            candidate: String,
            sdp_mid: Option<String>,
            sdp_m_line_index: Option<u16>,
        },
        #[serde(rename = "identify")]
        Identify {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            user_id: String,
            token: String,
        },

        // === Call Signaling ===
        /// Initiate a call to a target user
        #[serde(rename = "call_initiate")]
        CallInitiate {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
            public_key: String,
        },
        /// Incoming call notification (server -> callee)
        #[serde(rename = "incoming_call")]
        IncomingCall {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            caller_id: String,
            caller_name: String,
            public_key: String,
        },
        /// Accept an incoming call
        #[serde(rename = "call_accept")]
        CallAccept {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            caller_id: String,
            public_key: String,
        },
        /// Call was accepted (server -> caller)
        #[serde(rename = "call_accepted")]
        CallAccepted {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
            public_key: String,
        },
        /// Decline an incoming call
        #[serde(rename = "call_decline")]
        CallDecline {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            caller_id: String,
        },
        /// Call was declined (server -> caller)
        #[serde(rename = "call_declined")]
        CallDeclined {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
        },
        /// End an active call
        #[serde(rename = "call_end")]
        CallEnd {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            peer_id: String,
        },
        /// Call ended notification
        #[serde(rename = "call_ended")]
        CallEnded {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            peer_id: String,
        },
        /// Target user is busy (in another call)
        #[serde(rename = "call_busy")]
        CallBusy {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            caller_id: String,
        },
        /// Cancel outgoing call before answer
        #[serde(rename = "call_cancel")]
        CallCancel {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
        },
        /// Call was cancelled (server -> callee)
        #[serde(rename = "call_cancelled")]
        CallCancelled {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            caller_id: String,
        },
        /// Call cannot proceed (offline peer, expired ringing state, etc.)
        #[serde(rename = "call_unavailable")]
        CallUnavailable {
            #[serde(default = "default_message_version")]
            version: u8,
            #[serde(default)]
            trace_id: Option<String>,
            target_id: String,
            reason: String,
        },
    }

    impl SignalingMessage {
        pub fn version(&self) -> u8 {
            match self {
                SignalingMessage::Offer { version, .. }
                | SignalingMessage::Answer { version, .. }
                | SignalingMessage::Candidate { version, .. }
                | SignalingMessage::Identify { version, .. }
                | SignalingMessage::CallInitiate { version, .. }
                | SignalingMessage::IncomingCall { version, .. }
                | SignalingMessage::CallAccept { version, .. }
                | SignalingMessage::CallAccepted { version, .. }
                | SignalingMessage::CallDecline { version, .. }
                | SignalingMessage::CallDeclined { version, .. }
                | SignalingMessage::CallEnd { version, .. }
                | SignalingMessage::CallEnded { version, .. }
                | SignalingMessage::CallBusy { version, .. }
                | SignalingMessage::CallCancel { version, .. }
                | SignalingMessage::CallCancelled { version, .. }
                | SignalingMessage::CallUnavailable { version, .. } => *version,
            }
        }

        pub fn trace_id(&self) -> Option<&str> {
            match self {
                SignalingMessage::Offer { trace_id, .. }
                | SignalingMessage::Answer { trace_id, .. }
                | SignalingMessage::Candidate { trace_id, .. }
                | SignalingMessage::Identify { trace_id, .. }
                | SignalingMessage::CallInitiate { trace_id, .. }
                | SignalingMessage::IncomingCall { trace_id, .. }
                | SignalingMessage::CallAccept { trace_id, .. }
                | SignalingMessage::CallAccepted { trace_id, .. }
                | SignalingMessage::CallDecline { trace_id, .. }
                | SignalingMessage::CallDeclined { trace_id, .. }
                | SignalingMessage::CallEnd { trace_id, .. }
                | SignalingMessage::CallEnded { trace_id, .. }
                | SignalingMessage::CallBusy { trace_id, .. }
                | SignalingMessage::CallCancel { trace_id, .. }
                | SignalingMessage::CallCancelled { trace_id, .. }
                | SignalingMessage::CallUnavailable { trace_id, .. } => trace_id.as_deref(),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn legacy_messages_default_to_legacy_version() {
            let json = r#"{"type":"identify","payload":{"user_id":"u1","token":"jwt"}}"#;
            let parsed: SignalingMessage = serde_json::from_str(json).expect("parse signaling");
            assert_eq!(parsed.version(), LEGACY_PROTOCOL_VERSION);
        }

        #[test]
        fn version_and_trace_are_serialized() {
            let message = SignalingMessage::Offer {
                version: PROTOCOL_VERSION,
                trace_id: Some("trace-123".to_string()),
                target_id: "peer-2".to_string(),
                sdp: "sdp".to_string(),
            };

            let json = serde_json::to_string(&message).expect("serialize signaling");
            assert!(json.contains("\"version\":1"));
            assert!(json.contains("\"trace_id\":\"trace-123\""));
        }
    }
}

pub mod auth {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct LoginRequest {
        pub username: String,
        // Add more fields as needed
    }
}
