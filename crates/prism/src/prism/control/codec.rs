use bytes::Bytes;
use tokio_util::codec::LengthDelimitedCodec;

use super::types::{
    CONTROL_PROTO_V1, ControlError, ControlMsg, Envelope, HelloRejectReason,
    MAX_CONTROL_FRAME_BYTES,
};

pub fn length_codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .length_field_length(4)
        .max_frame_length(MAX_CONTROL_FRAME_BYTES)
        .new_codec()
}

pub fn encode_msg(msg: &ControlMsg) -> Result<Bytes, ControlError> {
    encode_envelope(&Envelope {
        proto: CONTROL_PROTO_V1,
        msg: postcard::to_allocvec(msg)?,
    })
}

pub fn encode_envelope(env: &Envelope) -> Result<Bytes, ControlError> {
    let bytes = postcard::to_allocvec(env)?;
    if bytes.len() > MAX_CONTROL_FRAME_BYTES {
        return Err(ControlError::FrameTooLarge);
    }
    Ok(Bytes::from(bytes))
}

pub fn decode_envelope(buf: &[u8]) -> Result<Envelope, ControlError> {
    if buf.len() > MAX_CONTROL_FRAME_BYTES {
        return Err(ControlError::FrameTooLarge);
    }
    Ok(postcard::from_bytes(buf)?)
}

pub fn decode_msg(env: &Envelope) -> Result<ControlMsg, ControlError> {
    if env.proto != CONTROL_PROTO_V1 {
        return Err(ControlError::UnsupportedVersion(env.proto));
    }
    Ok(postcard::from_bytes(&env.msg)?)
}

#[cfg(test)]
pub fn decode_frame(buf: &[u8]) -> Result<ControlMsg, ControlError> {
    decode_msg(&decode_envelope(buf)?)
}

pub fn reject_unsupported_version(peer: u16) -> Result<Bytes, ControlError> {
    encode_msg(&ControlMsg::HelloReject {
        reason: HelloRejectReason::UnsupportedVersion {
            peer,
            server: CONTROL_PROTO_V1,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prism::control::types::{Envelope, FEATURE_RPC};

    #[test]
    fn envelope_roundtrip() {
        let msg = ControlMsg::Hello {
            features: FEATURE_RPC,
        };
        let bytes = encode_msg(&msg).unwrap();
        let got = decode_frame(&bytes).unwrap();
        assert_eq!(got, msg);
    }

    #[test]
    fn unknown_proto_is_unsupported() {
        let env = Envelope {
            proto: 99,
            msg: vec![1, 2, 3],
        };
        let bytes = encode_envelope(&env).unwrap();
        let decoded = decode_envelope(&bytes).unwrap();
        let err = decode_msg(&decoded).unwrap_err();
        match err {
            ControlError::UnsupportedVersion(99) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
