use anyhow::{anyhow, Result};
use ethabi::{ParamType, Token};
use ethers::{
    abi::AbiEncode,
    signers::LocalWallet,
    types::{H160, U256, U64},
    utils::keccak256,
};
use starknet::core::types::FieldElement;
use std::fmt::Write;

pub fn decode_ens(name: &str) -> String {
    let mut labels: Vec<&str> = Vec::new();
    let mut idx = 0;
    loop {
        let len = name.as_bytes()[idx] as usize;
        if len == 0 {
            break;
        }
        labels.push(&name[idx + 1..idx + len + 1]);
        idx += len + 1;
    }

    labels.join(".")
}

pub fn to_eth_hex(felt: &FieldElement) -> String {
    let bytes = felt.to_bytes_be();
    let mut result = String::with_capacity(42);
    result.push_str("0x");

    let start = bytes.iter().position(|&x| x != 0).unwrap_or(bytes.len());

    for byte in &bytes[start..] {
        write!(&mut result, "{:02x}", byte).unwrap();
    }

    // Ensure the result is exactly 42 characters long (0x + 40 characters of hex)
    if result.len() < 42 {
        let required_padding = 42 - result.len();
        let padding: String = "0".repeat(required_padding);
        result.insert_str(2, &padding); // Insert right after "0x"
    }
    result
}

pub fn decode_data(data: &str) -> Result<String> {
    let data = data
        .strip_prefix("0x9061b923")
        .ok_or_else(|| anyhow!("Invalid prefix"))?;

    let data = hex::decode(data)?;

    let decoded = ethers::abi::decode(&[ParamType::Bytes, ParamType::Bytes], &data)?;

    let dns_encoded_name = decoded[0]
        .clone()
        .into_bytes()
        .ok_or_else(|| anyhow!("Invalid bytes"))?;

    let name = String::from_utf8(dns_encoded_name)?;

    let name = decode_ens(&name);

    Ok(name)
}

pub fn sign_message(
    wallet: LocalWallet,
    sender: &str,
    expires: u64,
    request_hash: Vec<u8>,
    result_hash: Vec<u8>,
    data: Vec<u8>,
) -> Result<String> {
    let encoded = ethers::abi::encode_packed(&[
        Token::Uint(U256::from(0x1900)),
        Token::Address(
            sender
                .parse::<H160>()
                .expect("Failed to parse sender address"),
        ),
        Token::FixedBytes(U64::from(expires).0[0].to_be_bytes().to_vec()),
        Token::FixedBytes(request_hash),
        Token::FixedBytes(result_hash),
    ])
    .unwrap();

    let message_hash = keccak256(encoded);

    let signature: ethers::types::Signature = wallet.sign_hash(message_hash.into()).unwrap();

    let signature_r = signature.r.encode();
    let signature_s = signature.s.encode();
    let signature_v = vec![signature.v.try_into().unwrap()];

    let signature = [signature_r, signature_s, signature_v].concat();

    let pl = format!(
        "0x{}",
        hex::encode(ethers::abi::encode(
            vec![
                Token::Bytes(data),
                Token::Uint(U256::from(expires)),
                Token::Bytes(signature),
            ]
            .as_slice()
        ))
    );
    Ok(pl)
}
