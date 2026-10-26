//! An actions sheet and the arguments of an action a player runs: decoding
//! never panics, and arguments that decode encode back to themselves.

#![no_main]

use enshwick::actions::sheet::{decode, decode_args, encode, encode_args};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let entries = decode(text);
    drop(decode(&encode(&entries)));
    if let Some(args) = decode_args(text) {
        assert_eq!(decode_args(&encode_args(&args)), Some(args));
    }
});
