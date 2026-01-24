
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
        Offer(Offer),
        Answer(Answer),
        Candidate(Candidate),
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
