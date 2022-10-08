use ls_qpack::decoder::Decoder;
use ls_qpack::encoder::Encoder;
use ls_qpack::StreamId;

const HEADERS: [(&str, &str); 3] = [(":status", "404"), ("foo", "bar"), ("foo", "bar")];

fn main() {
    let mut encoder = Encoder::new();
    let sdtc_data = encoder.configure(1024, 1024, 1).unwrap();

    let (encoded_hdr_data, encoded_stream_data) = encoder
        .encode_all(StreamId::new(0), HEADERS)
        .unwrap()
        .into();

    println!("Encoding ratio: {}", encoder.ratio());

    let mut decoder = Decoder::new(1024, 1);
    decoder.feed(sdtc_data).unwrap();

    let decoder_status = decoder.decode(StreamId::new(0), encoded_hdr_data).unwrap();

    assert!(decoder_status.is_blocked());
    println!("Decoder blocked. Stream data needed");

    decoder.feed(encoded_stream_data).unwrap();

    let decoded_hdr = decoder
        .unblocked(StreamId::new(0))
        .unwrap()
        .unwrap()
        .take()
        .unwrap();

    println!("Decoded header: {:?}", decoded_hdr);
}
