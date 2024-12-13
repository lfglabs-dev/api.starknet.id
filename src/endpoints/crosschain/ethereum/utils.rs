use anyhow::{anyhow, Result};
use ethabi::{ParamType, Token};
use ethers::{
    abi::AbiEncode,
    signers::LocalWallet,
    types::{H160, U256, U64},
    utils::keccak256,
};
use futures::StreamExt;
use lazy_static::lazy_static;
use mongodb::{
    bson::{doc, from_document, Document},
    Collection,
};
use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::parse_cairo_short_string,
    },
    macros::selector,
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};
use std::fmt::Write;

use crate::{
    logger::Logger , 
    config , 
    config::Config,
    endpoints::uri::VerifierData,
    utils::{fetch_image_url, parse_base64_image, to_hex},
};

use super::lookup::ResolverFunctionCall;

lazy_static! {
    static ref NFT_PP_CONTRACT: String =
        "0x00000000000000000000000000000000006e66745f70705f636f6e7472616374".to_string();
    static ref NFT_PP_ID: String =
        "0x00000000000000000000000000000000000000000000006e66745f70705f6964".to_string();
}

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

pub fn decode_data(data: &str) -> Result<(String, ResolverFunctionCall)> {
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

    let rest_of_the_data = decoded[1]
        .clone()
        .into_bytes()
        .ok_or_else(|| anyhow!("Invalid bytes"))?;

    let data = ResolverFunctionCall::try_from(rest_of_the_data.as_slice())?;

    Ok((name, data))
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

// user data utils
pub async fn get_user_data(
    provider: &JsonRpcClient<HttpTransport>,
    contract: FieldElement,
    id: FieldElement,
    field: FieldElement,
) -> Option<FieldElement> {
    let conf = config::load();
    let logger = Logger::new(&conf.watchtower);
    let call_result = provider
        .call(
            FunctionCall {
                contract_address: contract,
                entry_point_selector: selector!("get_user_data"),
                calldata: vec![
                    id,
                    // cairo_short_string_to_felt(field).unwrap(),
                    field,
                    FieldElement::ZERO,
                ],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await;

    match call_result {
        Ok(result) => {
            if result[0] != FieldElement::ZERO {
                return Some(result[0]);
            }
            None
        }
        Err(e) => {
            logger.severe(format!("Error fetching user data for field {} : {}", field, e));
            None
        }
    }
}

// argent multicall to fetch both fields at once
pub async fn get_user_data_multicall(
    provider: &JsonRpcClient<HttpTransport>,
    config: &Config,
    id: FieldElement,
    fields: Vec<FieldElement>,
) -> Option<FieldElement> {
    let conf = config::load();
    let logger = Logger::new(&conf.watchtower);
    let mut calls: Vec<FieldElement> = vec![FieldElement::from(fields.len())];
    for field in fields {
        calls.push(config.contracts.starknetid);
        calls.push(selector!("get_user_data"));
        calls.push(FieldElement::THREE);
        calls.push(id);
        calls.push(field);
        calls.push(FieldElement::ZERO)
    }
    let call_result = provider
        .call(
            FunctionCall {
                contract_address: config.contracts.argent_multicall,
                entry_point_selector: selector!("aggregate"),
                calldata: calls,
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await;

    match call_result {
        Ok(result) => {
            if result[3] != FieldElement::ZERO {
                Some(result[3])
            } else if result[5] != FieldElement::ZERO {
                Some(result[5])
            } else {
                None
            }
        }
        Err(err) => {
            logger.severe(format!("Error while fetching balances: {:?}", err));
            None
        }
    }
}

pub async fn domain_to_address(
    provider: &JsonRpcClient<HttpTransport>,
    naming_contract: FieldElement,
    encoded_domain: Vec<FieldElement>,
) -> Option<FieldElement> {
    let conf = config::load();
    let logger = Logger::new(&conf.watchtower);
    let mut calldata: Vec<FieldElement> = vec![FieldElement::from(encoded_domain.len())];
    calldata.extend(encoded_domain);
    calldata.push(FieldElement::ZERO);
    let call_result = provider
        .call(
            FunctionCall {
                contract_address: naming_contract,
                entry_point_selector: selector!("domain_to_address"),
                calldata,
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await;

    match call_result {
        Ok(result) => {
            logger.info(format!("domain_to_address result: {:?}", result));
            if result[0] != FieldElement::ZERO {
                return Some(result[0]);
            }
            None
        }
        Err(e) => {
            logger.severe(format!("Error fetching starknet address for domain : {}", e));
            None
        }
    }
}

// Profile picture metadata utils
pub async fn get_profile_picture(
    config: &Config,
    provider: &JsonRpcClient<HttpTransport>,
    verifier_data_collection: Collection<Document>,
    pfp_verifier: FieldElement,
    id: FieldElement,
) -> Option<String> {
    let conf = config::load();
    let logger = Logger::new(&conf.watchtower);
    let pipeline: Vec<Document> = vec![doc! {
        "$match": {
            "_cursor.to": null,
            "id": to_hex(&id),
            "verifier": to_hex(&pfp_verifier)
        }
    }];
    match verifier_data_collection.aggregate(pipeline, None).await {
        Ok(mut cursor) => {
            let mut contract_addr: String = String::new();
            let mut token_id: (String, String) = (String::new(), String::new());
            while let Some(result) = cursor.next().await {
                if let Ok(doc) = result {
                    if let Ok(verifier_data) = from_document::<VerifierData>(doc) {
                        if *verifier_data.field == *NFT_PP_CONTRACT {
                            if let Some(addr) = verifier_data.data {
                                contract_addr = addr;
                            }
                        } else if *verifier_data.field == *NFT_PP_ID {
                            if let Some(token_id_arr) = verifier_data.extended_data {
                                token_id = (token_id_arr[0].clone(), token_id_arr[1].clone());
                            }
                        } else {
                            logger.warning(format!("Error: failed to get 'extended_data' as array"));
                        }
                    }
                }
            }
            if contract_addr.is_empty() || token_id.0.is_empty() {
                return None;
            }

            // we fetch the tokenURI from the contract
            let call_result = provider
                .call(
                    FunctionCall {
                        contract_address: FieldElement::from_hex_be(&contract_addr).unwrap(),
                        entry_point_selector: selector!("tokenURI"),
                        calldata: vec![
                            FieldElement::from_hex_be(&token_id.0).unwrap(),
                            FieldElement::from_hex_be(&token_id.1).unwrap(),
                        ],
                    },
                    BlockId::Tag(BlockTag::Latest),
                )
                .await;
            match call_result {
                Ok(result) => {
                    let pfp_metadata = result
                        .iter()
                        .skip(1)
                        .filter_map(|val| parse_cairo_short_string(val).ok())
                        .collect::<Vec<String>>() // Collect into a vector of strings
                        .join("");
                    match get_profile_picture_uri(
                        config,
                        Some(&pfp_metadata),
                        true,
                        &id.to_string(),
                    )
                    .await
                    {
                        Some(pfp) => {
                            logger.info(format!("Profile picture fetched successfully {}", pfp));
                            Some(pfp)
                        }
                        None => {
                            logger.warning(format!("Error fetching profile picture from tokenURI"));
                            None
                        }
                    }
                }
                Err(e) => {
                    logger.severe(format!("Error fetching tokenURI for token : {}", e));
                    None
                }
            }
        }
        Err(_) => {
            logger.severe(format!("Error while fetching profile picture from database"));
            None
        }
    }
}

pub async fn get_profile_picture_uri(
    config: &Config,
    uri: Option<&str>,
    use_default_pfp: bool,
    id: &str,
) -> Option<String> {
    match uri {
        Some(u) if u.contains("base64") => Some(parse_base64_image(u)),
        Some(u) => Some(fetch_image_url(config, u).await),
        None if use_default_pfp => Some(format!("https://identicon.starknet.id/{}", id)),
        _ => None,
    }
}
