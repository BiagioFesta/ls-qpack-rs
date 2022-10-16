# ls-qpack
[![crates.io](https://img.shields.io/crates/v/ls-qpack.svg)](https://crates.io/crates/ls-qpack)
[![docs.rs](https://docs.rs/ls-qpack/badge.svg)](https://docs.rs/ls-qpack)




**QPACK**: Field Compression for HTTP/3 
([*RFC 9204*](https://datatracker.ietf.org/doc/rfc9204/))

Rust implementation based on [ls-qpack](https://github.com/litespeedtech/ls-qpack)

## Introduction
QPACK is a compressor for headers data used by [HTTP/3](https://en.wikipedia.org/wiki/HTTP/3). 
It allows correctness in the presence of out-of-order delivery, 
with flexibility for implementations to balance between resilience against 
head-of-line blocking and optimal compression ratio.

## Example
```rust
use ls_qpack::decoder::Decoder;
use ls_qpack::encoder::Encoder;
use ls_qpack::StreamId;

const HEADERS: [(&str, &str); 2] = [(":status", "404"), ("foo", "bar")];

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
```
