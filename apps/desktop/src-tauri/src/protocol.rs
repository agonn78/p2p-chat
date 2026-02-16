pub const PROTOCOL_VERSION: u8 = shared_proto::signaling::PROTOCOL_VERSION;
pub const LEGACY_PROTOCOL_VERSION: u8 = shared_proto::signaling::LEGACY_PROTOCOL_VERSION;

pub const HEADER_PROTOCOL_VERSION: &str = "X-Protocol-Version";
pub const HEADER_TRACE_ID: &str = "X-Trace-Id";
pub const HEADER_REQUEST_ID: &str = "X-Request-Id";

pub fn is_supported_protocol_version(version: u8) -> bool {
    shared_proto::signaling::is_supported_protocol_version(version)
}
