use byteorder::WriteBytesExt;
use bytes::{BufMut, BytesMut};

use crate::chunk::{ChunkDecodeError, ChunkDecoder};

#[test]
fn test_decoder_error_display() {
    let error = ChunkDecodeError::IO(std::io::Error::other("test"));
    assert_eq!(format!("{}", error), "io error: test");

    let error = ChunkDecodeError::InvalidChunkType(123);
    assert_eq!(format!("{}", error), "invalid chunk type: 123");

    let error = ChunkDecodeError::InvalidMessageTypeID(123);
    assert_eq!(format!("{}", error), "invalid message type id: 123");

    let error = ChunkDecodeError::MissingPreviousChunkHeader(123);
    assert_eq!(format!("{}", error), "missing previous chunk header: 123");

    let error = ChunkDecodeError::TooManyPartialChunks;
    assert_eq!(format!("{}", error), "too many partial chunks");

    let error = ChunkDecodeError::TooManyPreviousChunkHeaders;
    assert_eq!(format!("{}", error), "too many previous chunk headers");

    let error = ChunkDecodeError::PartialChunkTooLarge(100);
    assert_eq!(format!("{}", error), "partial chunk too large: 100");

    let error = ChunkDecodeError::TimestampOverflow(100, 200);
    assert_eq!(format!("{}", error), "timestamp overflow: timestamp: 100, delta: 200");
}

#[test]
fn test_decoder_chunk_type0_single_sized() {
    let mut buf = BytesMut::new();

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0x00, 0x00, 0x00, // timestamp
        0x00, 0x00, 0x80, // message length (128) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let mut unpacker = ChunkDecoder::default();
    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");
    assert_eq!(chunk.basic_header.chunk_stream_id, 3);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x09);
    assert_eq!(chunk.message_header.timestamp, 0);
    assert_eq!(chunk.message_header.msg_length, 128);
    assert_eq!(chunk.message_header.msg_stream_id, 0x0100); // since it's little endian, it's 0x0100
    assert_eq!(chunk.payload.len(), 128);
}

#[test]
fn test_decoder_chunk_type0_double_sized() {
    let mut buf = BytesMut::new();
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0x00, 0x00, 0x00, // timestamp
        0x00, 0x01, 0x00, // message length (256) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let mut unpacker = ChunkDecoder::default();

    let chunk = buf.as_ref().to_vec();

    // We should not have enough data to read the chunk
    // But the chunk is valid, so we should not get an error
    assert!(unpacker.read_chunk(&mut buf).expect("read chunk").is_none());

    // We just feed the same data again in this test to see if the Unpacker merges
    // the chunks Which it should do
    buf.extend_from_slice(&chunk);

    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 3);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x09);
    assert_eq!(chunk.message_header.timestamp, 0);
    assert_eq!(chunk.message_header.msg_length, 256);
    assert_eq!(chunk.message_header.msg_stream_id, 0x0100); // since it's little endian, it's 0x0100
    assert_eq!(chunk.payload.len(), 256);
}

#[test]
fn test_decoder_chunk_mutli_streams() {
    let mut buf = BytesMut::new();

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0x00, 0x00, 0x00, // timestamp
        0x00, 0x01, 0x00, // message length (256) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
    ]);

    for _ in 0..128 {
        (&mut buf).writer().write_u8(3).unwrap();
    }

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        4, // chunk type 0, chunk stream id 4 (different stream)
        0x00, 0x00, 0x00, // timestamp
        0x00, 0x01, 0x00, // message length (256) (max chunk size is set to 128)
        0x08, // message type id (audio)
        0x00, 0x03, 0x00, 0x00, // message stream id
    ]);

    for _ in 0..128 {
        (&mut buf).writer().write_u8(4).unwrap();
    }

    let mut unpacker = ChunkDecoder::default();

    // We wrote 2 chunks but neither of them are complete
    assert!(unpacker.read_chunk(&mut buf).expect("read chunk").is_none());

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (3 << 6) | 4, // chunk type 3, chunk stream id 4
    ]);

    for _ in 0..128 {
        (&mut buf).writer().write_u8(3).unwrap();
    }

    // Even though we wrote chunk 3 first, chunk 4 should be read first since it's a
    // different stream
    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 4);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x08);
    assert_eq!(chunk.message_header.timestamp, 0);
    assert_eq!(chunk.message_header.msg_length, 256);
    assert_eq!(chunk.message_header.msg_stream_id, 0x0300); // since it's little endian, it's 0x0100
    assert_eq!(chunk.payload.len(), 256);
    for i in 0..128 {
        assert_eq!(chunk.payload[i], 4);
    }

    // No chunk is ready yet
    assert!(unpacker.read_chunk(&mut buf).expect("read chunk").is_none());

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (3 << 6) | 3, // chunk type 3, chunk stream id 3
    ]);

    for _ in 0..128 {
        (&mut buf).writer().write_u8(3).unwrap();
    }

    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 3);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x09);
    assert_eq!(chunk.message_header.timestamp, 0);
    assert_eq!(chunk.message_header.msg_length, 256);
    assert_eq!(chunk.message_header.msg_stream_id, 0x0100); // since it's little endian, it's 0x0100
    assert_eq!(chunk.payload.len(), 256);
    for i in 0..128 {
        assert_eq!(chunk.payload[i], 3);
    }
}

#[test]
fn test_decoder_extended_timestamp() {
    let mut buf = BytesMut::new();

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0xFF, 0xFF, 0xFF, // timestamp
        0x00, 0x02, 0x00, // message length (384) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
        0x01, 0x00, 0x00, 0x00, // extended timestamp
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let mut unpacker = ChunkDecoder::default();

    // We should not have enough data to read the chunk
    // But the chunk is valid, so we should not get an error
    assert!(unpacker.read_chunk(&mut buf).expect("read chunk").is_none());

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (1 << 6) | 3, // chunk type 1, chunk stream id 3
        0xFF, 0xFF, 0xFF, // extended timestamp (again)
        0x00, 0x02, 0x00, // message length (384) (max chunk size is set to 128)
        0x09, // message type id (video)
        // message stream id is not present since it's the same as the previous chunk
        0x01, 0x00, 0x00, 0x00, // extended timestamp (again)
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (2 << 6) | 3, // chunk type 3, chunk stream id 3
        0x00, 0x00, 0x01, // not extended timestamp
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (3 << 6) | 3, // chunk type 3, chunk stream id 3
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 3);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x09);
    assert_eq!(chunk.message_header.timestamp, 0x02000001);
    assert_eq!(chunk.message_header.msg_length, 512);
    assert_eq!(chunk.message_header.msg_stream_id, 0x0100); // since it's little endian, it's 0x0100
    assert_eq!(chunk.payload.len(), 512);
}

#[test]
fn test_decoder_extended_timestamp_ext() {
    let mut buf = BytesMut::new();

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0xFF, 0xFF, 0xFF, // timestamp
        0x00, 0x01, 0x00, // message length (256) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
        0x01, 0x00, 0x00, 0x00, // extended timestamp
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let mut unpacker = ChunkDecoder::default();

    // We should not have enough data to read the chunk
    // But the chunk is valid, so we should not get an error
    assert!(unpacker.read_chunk(&mut buf).expect("read chunk").is_none());

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (3 << 6) | 3, // chunk type 1, chunk stream id 3
        0x00, 0x00, 0x00, 0x00, // extended timestamp this value is ignored
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 3);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x09);
    assert_eq!(chunk.message_header.timestamp, 0x01000000);
    assert_eq!(chunk.message_header.msg_length, 256);
    assert_eq!(chunk.message_header.msg_stream_id, 0x0100); // since it's little endian, it's 0x0100
    assert_eq!(chunk.payload.len(), 256);
}

#[test]
fn test_read_extended_csid() {
    let mut buf = BytesMut::new();

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (0 << 6), // chunk type 0, chunk stream id 0
        10,       // extended chunk stream id
        0x00, 0x00, 0x00, // timestamp
        0x00, 0x00, 0x00, // message length (256) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
    ]);

    let mut unpacker = ChunkDecoder::default();
    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 64 + 10);
}

#[test]
fn test_read_extended_csid_ext2() {
    let mut buf = BytesMut::new();

    #[rustfmt::skip]
    buf.extend_from_slice(&[
        1,  // chunk type 0, chunk stream id 0
        10, // extended chunk stream id
        13, // extended chunk stream id 2
        0x00, 0x00, 0x00, // timestamp
        0x00, 0x00, 0x00, // message length (256) (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
    ]);

    let mut unpacker = ChunkDecoder::default();

    let chunk = unpacker.read_chunk(&mut buf).expect("read chunk").expect("chunk");

    assert_eq!(chunk.basic_header.chunk_stream_id, 64 + 10 + 256 * 13);
}

#[test]
fn test_decoder_error_no_previous_chunk() {
    let mut buf = BytesMut::new();

    // Write a chunk with type 3 but no previous chunk
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        (3 << 6) | 3, // chunk type 0, chunk stream id 3
    ]);

    let mut unpacker = ChunkDecoder::default();
    let err = unpacker.read_chunk(&mut buf).unwrap_err();
    match err {
        ChunkDecodeError::MissingPreviousChunkHeader(3) => {}
        _ => panic!("Unexpected error: {:?}", err),
    }
}

#[test]
fn test_decoder_error_partial_chunk_too_large() {
    let mut buf = BytesMut::new();

    // Write a chunk that has a message size that is too large
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0xFF, 0xFF, 0xFF, // timestamp
        0xFF, 0xFF, 0xFF, // message length (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
        0x01, 0x00, 0x00, 0x00, // extended timestamp
    ]);

    let mut unpacker = ChunkDecoder::default();

    let err = unpacker.read_chunk(&mut buf).unwrap_err();
    match err {
        ChunkDecodeError::PartialChunkTooLarge(16777215) => {}
        _ => panic!("Unexpected error: {:?}", err),
    }
}

#[test]
fn test_decoder_error_invalid_message_type_id() {
    let mut buf = BytesMut::new();

    // Write a chunk with an invalid message type id
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0xFF, 0xFF, 0xFF, // timestamp
        0x08, 0x00, 0x00, // message length (max chunk size is set to 128)
        0xFF, // message type id (invalid)
        0x00, 0x01, 0x00, 0x00, // message stream id
        0x01, 0x00, 0x00, 0x00, // extended timestamp
    ]);

    let mut unpacker = ChunkDecoder::default();

    let err = unpacker.read_chunk(&mut buf).unwrap_err();

    match err {
        ChunkDecodeError::InvalidMessageTypeID(0xFF) => {}
        _ => panic!("Unexpected error: {:?}", err),
    }
}

#[test]
fn test_decoder_error_too_many_partial_chunks() {
    let mut buf = BytesMut::new();

    let mut unpacker = ChunkDecoder::default();

    for i in 0..4 {
        // Write another chunk with a different chunk stream id
        #[rustfmt::skip]
        buf.extend_from_slice(&[
            (i + 2), // chunk type 0 (partial), chunk stream id i
            0xFF, 0xFF, 0xFF, // timestamp
            0x00, 0x01, 0x00, // message length (max chunk size is set to 128)
            0x09, // message type id (video)
            0x00, 0x01, 0x00, 0x00, // message stream id
            0x01, 0x00, 0x00, 0x00, // extended timestamp
        ]);

        for i in 0..128 {
            (&mut buf).writer().write_u8(i as u8).unwrap();
        }

        // Read the chunk
        assert!(
            unpacker
                .read_chunk(&mut buf)
                .unwrap_or_else(|_| panic!("chunk failed {}", i))
                .is_none()
        );
    }

    // Write another chunk with a different chunk stream id
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        12, // chunk type 0, chunk stream id 6
        0xFF, 0xFF, 0xFF, // timestamp
        0x00, 0x01, 0x00, // message length (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
        0x01, 0x00, 0x00, 0x00, // extended timestamp
    ]);

    for i in 0..128 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let err = unpacker.read_chunk(&mut buf).unwrap_err();
    match err {
        ChunkDecodeError::TooManyPartialChunks => {}
        _ => panic!("Unexpected error: {:?}", err),
    }
}

#[test]
fn test_decoder_error_too_many_chunk_headers() {
    let mut buf = BytesMut::new();

    let mut unpacker = ChunkDecoder::default();

    for i in 0..100 {
        // Write another chunk with a different chunk stream id
        #[rustfmt::skip]
        buf.extend_from_slice(&[
            (0 << 6), // chunk type 0 (partial), chunk stream id 0
            i,        // chunk id
            0xFF, 0xFF, 0xFF, // timestamp
            0x00, 0x00, 0x00, // message length (max chunk size is set to 128)
            0x09, // message type id (video)
            0x00, 0x01, 0x00, 0x00, // message stream id
            0x01, 0x00, 0x00, 0x00, // extended timestamp
        ]);

        // Read the chunk (should be a full chunk since the message length is 0)
        assert!(
            unpacker
                .read_chunk(&mut buf)
                .unwrap_or_else(|_| panic!("chunk failed {}", i))
                .is_some()
        );
    }

    // Write another chunk with a different chunk stream id
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        12, // chunk type 0, chunk stream id 6
        0xFF, 0xFF, 0xFF, // timestamp
        0x00, 0x00, 0x00, // message length (max chunk size is set to 128)
        0x09, // message type id (video)
        0x00, 0x01, 0x00, 0x00, // message stream id
        0x01, 0x00, 0x00, 0x00, // extended timestamp
    ]);

    let err = unpacker.read_chunk(&mut buf).unwrap_err();
    match err {
        ChunkDecodeError::TooManyPreviousChunkHeaders => {}
        _ => panic!("Unexpected error: {:?}", err),
    }
}

#[test]
fn test_decoder_larger_chunk_size() {
    let mut buf = BytesMut::new();

    // Write a chunk that has a message size that is too large
    #[rustfmt::skip]
    buf.extend_from_slice(&[
        3, // chunk type 0, chunk stream id 3
        0x00, 0x00, 0xFF, // timestamp
        0x00, 0x0F, 0x00, // message length ()
        0x09, // message type id (video)
        0x01, 0x00, 0x00, 0x00, // message stream id
    ]);

    for i in 0..3840 {
        (&mut buf).writer().write_u8(i as u8).unwrap();
    }

    let mut unpacker = ChunkDecoder::default();
    unpacker.update_max_chunk_size(4096);

    let chunk = unpacker.read_chunk(&mut buf).expect("failed").expect("chunk");
    assert_eq!(chunk.basic_header.chunk_stream_id, 3);
    assert_eq!(chunk.message_header.timestamp, 255);
    assert_eq!(chunk.message_header.msg_length, 3840);
    assert_eq!(chunk.message_header.msg_type_id as u8, 0x09);
    assert_eq!(chunk.message_header.msg_stream_id, 1); // little endian
    assert_eq!(chunk.payload.len(), 3840);

    for i in 0..3840 {
        assert_eq!(chunk.payload[i], i as u8);
    }
}
