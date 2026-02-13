pub mod signaling {
    use serde::{Deserialize, Serialize};

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
        Offer { target_id: String, sdp: String },
        #[serde(rename = "answer")]
        Answer { target_id: String, sdp: String },
        #[serde(rename = "candidate")]
        Candidate {
            target_id: String,
            candidate: String,
            sdp_mid: Option<String>,
            sdp_m_line_index: Option<u16>,
        },
        #[serde(rename = "identify")]
        Identify { user_id: String, token: String },

        // === Call Signaling ===
        /// Initiate a call to a target user
        #[serde(rename = "call_initiate")]
        CallInitiate {
            target_id: String,
            public_key: String,
        },
        /// Incoming call notification (server -> callee)
        #[serde(rename = "incoming_call")]
        IncomingCall {
            caller_id: String,
            caller_name: String,
            public_key: String,
        },
        /// Accept an incoming call
        #[serde(rename = "call_accept")]
        CallAccept {
            caller_id: String,
            public_key: String,
        },
        /// Call was accepted (server -> caller)
        #[serde(rename = "call_accepted")]
        CallAccepted {
            target_id: String,
            public_key: String,
        },
        /// Decline an incoming call
        #[serde(rename = "call_decline")]
        CallDecline { caller_id: String },
        /// Call was declined (server -> caller)
        #[serde(rename = "call_declined")]
        CallDeclined { target_id: String },
        /// End an active call
        #[serde(rename = "call_end")]
        CallEnd { peer_id: String },
        /// Call ended notification
        #[serde(rename = "call_ended")]
        CallEnded { peer_id: String },
        /// Target user is busy (in another call)
        #[serde(rename = "call_busy")]
        CallBusy { caller_id: String },
        /// Cancel outgoing call before answer
        #[serde(rename = "call_cancel")]
        CallCancel { target_id: String },
        /// Call was cancelled (server -> callee)
        #[serde(rename = "call_cancelled")]
        CallCancelled { caller_id: String },
        /// Call cannot proceed (offline peer, expired ringing state, etc.)
        #[serde(rename = "call_unavailable")]
        CallUnavailable { target_id: String, reason: String },
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
