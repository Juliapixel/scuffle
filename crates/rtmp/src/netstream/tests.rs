use std::collections::HashMap;

use amf0::{Amf0Reader, Amf0Value, Amf0WriteError};
use bytes::{BufMut, BytesMut};

use crate::chunk::{ChunkDecoder, ChunkEncodeError, ChunkEncoder};
use crate::netstream::{NetStreamError, NetStreamWriter};

#[test]
fn test_error_display() {
    let error = NetStreamError::Amf0Write(Amf0WriteError::NormalStringTooLong);
    assert_eq!(error.to_string(), "amf0 write error: normal string too long");

    let error = NetStreamError::ChunkEncode(ChunkEncodeError::UnknownReadState);
    assert_eq!(error.to_string(), "chunk encode error: unknown read state");
}

#[test]
fn test_netstream_write_on_status() {
    let encoder = ChunkEncoder::default();
    let mut buf = BytesMut::new();

    NetStreamWriter::write_on_status(&encoder, &mut (&mut buf).writer(), 1.0, "status", "idk", "description").unwrap();

    let mut decoder = ChunkDecoder::default();

    let chunk = decoder.read_chunk(&mut buf).expect("read chunk").expect("chunk");
    assert_eq!(chunk.basic_header.chunk_stream_id, 0x03);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x14);
    assert_eq!(chunk.message_header.msg_stream_id, 0);

    let mut amf0_reader = Amf0Reader::new(chunk.payload);
    let values = amf0_reader.read_all().unwrap();

    assert_eq!(values.len(), 4);
    assert_eq!(values[0], Amf0Value::String("onStatus".to_string())); // command name
    assert_eq!(values[1], Amf0Value::Number(1.0)); // transaction id
    assert_eq!(values[2], Amf0Value::Null); // command object
    assert_eq!(
        values[3],
        Amf0Value::Object(HashMap::from([
            ("code".to_string(), Amf0Value::String("idk".to_string())),
            ("level".to_string(), Amf0Value::String("status".to_string())),
            ("description".to_string(), Amf0Value::String("description".to_string())),
        ]))
    ); // info object
}
