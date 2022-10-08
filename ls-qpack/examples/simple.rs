use ls_qpack::decoder::Decoder;
use ls_qpack::encoder::Encoder;
use ls_qpack::StreamId;

const HEADERS: [(&str, &str); 2] = [(":status", "404"), ("pippo", "paperino")];

fn main() {
    let (encoded_headers, _) = Encoder::new()
        .encode_all(StreamId::new(0), HEADERS)
        .unwrap()
        .into();

    let decoded_headers = Decoder::new(0, 0)
        .decode(StreamId::new(0), encoded_headers)
        .unwrap()
        .take()
        .unwrap();

    println!("Decoded header: {:?}", decoded_headers);
}
