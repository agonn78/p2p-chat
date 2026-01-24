
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
        Offer {
            target_id: String,
            sdp: String,
        },
        #[serde(rename = "answer")]
        Answer {
            target_id: String,
            sdp: String,
        },
        #[serde(rename = "candidate")]
        Candidate {
            target_id: String,
            candidate: String,
            sdp_mid: Option<String>,
            sdp_m_line_index: Option<u16>,
        },
        #[serde(rename = "identify")]
        Identify {
            user_id: String,
        },
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
