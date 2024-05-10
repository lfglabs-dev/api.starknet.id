use std::{str::FromStr, sync::Arc};

use crate::{
    endpoints::crosschain::ethereum::{
        lookup::ResolverFunctionCall,
        utils::{
            decode_data, domain_to_address, get_profile_picture, get_user_data,
            get_user_data_multicall, sign_message, to_eth_hex,
        },
    },
    models::AppState,
    utils::{get_error, to_hex},
};
use anyhow::Result;
use axum::{
    async_trait,
    body::Body,
    extract::{FromRequest, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_auto_routes::route;
use bytes::{BufMut, BytesMut};
use ethabi::Token;
use ethers::{signers::LocalWallet, types::H160, utils::keccak256};
use futures::{pin_mut, stream::StreamExt as _};
use lazy_static::lazy_static;
use mongodb::bson::doc;
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use starknet::{
    core::types::{BlockId, BlockTag, FieldElement, FunctionCall},
    macros::{selector, short_string},
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};
use starknet_id::encode;

#[derive(Deserialize, Debug, Clone)]
pub struct ResolveQuery {
    data: String,   // data encoded
    sender: String, // resolver contract address
}

pub enum Query {
    Json(ResolveQuery),
    Form(ResolveQuery),
}

pub struct CustomRejection(String);

impl IntoResponse for CustomRejection {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.0).into_response()
    }
}

#[async_trait]
impl<S> FromRequest<S, Body> for Query
where
    S: Send + Sync + 'static,
{
    type Rejection = CustomRejection;

    async fn from_request(req: Request<Body>, _state: &S) -> Result<Self, Self::Rejection> {
        let body = req.into_body();
        pin_mut!(body); // Pin the body stream to the stack

        let mut bytes = BytesMut::new();

        // Collecting data chunks from the body stream
        while let Some(chunk) = body.next().await {
            let chunk =
                chunk.map_err(|_| CustomRejection("Failed to read request body".to_string()))?;
            bytes.put(chunk);
        }

        let full_body = bytes.freeze(); // Convert collected data into Bytes

        // Attempt to parse as JSON
        if let Ok(json_data) = serde_json::from_slice::<ResolveQuery>(&full_body) {
            return Ok(Query::Json(json_data));
        }

        // Attempt to parse as Form data
        if let Ok(form_data) = serde_urlencoded::from_bytes::<ResolveQuery>(&full_body) {
            return Ok(Query::Form(form_data));
        }

        Err(CustomRejection("Unsupported Content Type".to_string()))
    }
}

lazy_static! {
    static ref EVM_ADDRESS: FieldElement = short_string!("evm-address");
    static ref ETHEREUM: FieldElement = short_string!("ethereum");
}

#[route(
    post,
    "/crosschain/ethereum/resolve",
    crate::endpoints::crosschain::ethereum::resolve
)]
pub async fn handler(State(state): State<Arc<AppState>>, query: Query) -> impl IntoResponse {
    let (encoded_data, sender) = match query {
        Query::Json(data) => (data.data, data.sender.to_lowercase()),
        Query::Form(data) => (data.data, data.sender.to_lowercase()),
    };

    match decode_data(&encoded_data) {
        Ok((name, resolver_function_call)) => {
            let parts: Vec<&str> = name.split('.').collect();
            let root_domain = if parts.len() > 2 {
                &parts[..parts.len() - 2]
            } else {
                return get_error(format!("Domain with wrong size {}", name));
            };
            let encoded_domain: Vec<FieldElement> = root_domain
                .iter()
                .map(|&part| encode(part).unwrap())
                .collect();

            // get the id of the domain
            let provider = JsonRpcClient::new(HttpTransport::new(
                Url::parse(&state.conf.variables.rpc_url).unwrap(),
            ));
            let mut calldata: Vec<FieldElement> = vec![FieldElement::from(encoded_domain.len())];
            calldata.extend(encoded_domain.clone());
            let call_result = provider
                .call(
                    FunctionCall {
                        contract_address: state.conf.contracts.naming,
                        entry_point_selector: selector!("domain_to_id"),
                        calldata,
                    },
                    BlockId::Tag(BlockTag::Latest),
                )
                .await;
            match call_result {
                Ok(result) => {
                    if result[0] == FieldElement::ZERO {
                        return get_error(format!("No identity found for : {}", name));
                    }
                    let id: FieldElement = result[0];

                    let payload: Vec<Token> = match resolver_function_call {
                        ResolverFunctionCall::Text(_alt_hash, record) => {
                            match record.as_str() {
                                // Records available "com.discord" "com.github" "com.twitter"
                                "url" => {
                                    match get_profile_picture(
                                        &state.conf,
                                        &provider,
                                        state.starknetid_db.collection::<mongodb::bson::Document>(
                                            "id_verifier_data",
                                        ),
                                        state.conf.contracts.pp_verifier,
                                        id,
                                    )
                                    .await
                                    {
                                        Some(pfp) => vec![Token::String(pfp)],
                                        None => {
                                            return get_error(
                                                "No profile picture specified for this domain"
                                                    .to_string(),
                                            )
                                        }
                                    }
                                }
                                _ => {
                                    println!("Record not implemented: {:?}", record);
                                    return get_error(format!("Record not implemented {}", record));
                                }
                            }
                        }
                        ResolverFunctionCall::AddrMultichain(_bf, chain) => {
                            println!("AddrMultichain for chain: {:?}", chain);

                            // EVM chains have an id >=  0x80000000 (2147483648)
                            if chain >= 2147483648 {
                                if chain == 2147492652 {
                                    // Starknet chain id, we fetch the user address from the domain
                                    print!("Fetch Starknet address");
                                    match domain_to_address(
                                        &provider,
                                        state.conf.contracts.naming,
                                        encoded_domain,
                                    )
                                    .await
                                    {
                                        Some(addr) => {
                                            let hex_addr = to_hex(&addr);
                                            let trimmed_hex_addr =
                                                hex_addr.trim_start_matches("0x");
                                            let bytes =
                                                ethers::utils::hex::decode(trimmed_hex_addr)
                                                    .map_err(|err| {
                                                        get_error(format!(
                                                            "Invalid Structure: {}",
                                                            err
                                                        ))
                                                    })
                                                    .unwrap();
                                            vec![Token::Bytes(bytes)]
                                        }
                                        None => {
                                            return get_error(
                                                "No starknet address specified for this domain"
                                                    .to_string(),
                                            );
                                        }
                                    }
                                } else {
                                    // evm chain
                                    match state.conf.evm_networks.get(&chain) {
                                        Some(field_name) => {
                                            match get_user_data_multicall(
                                                &provider,
                                                &state.conf,
                                                id,
                                                vec![*field_name, *EVM_ADDRESS],
                                            )
                                            .await
                                            {
                                                Some(addr) => {
                                                    let hex_addr = to_eth_hex(&addr);
                                                    let trimmed_hex_addr =
                                                        hex_addr.trim_start_matches("0x");
                                                    let bytes = ethers::utils::hex::decode(
                                                        trimmed_hex_addr,
                                                    )
                                                    .map_err(|err| {
                                                        get_error(format!(
                                                            "Invalid Structure: {}",
                                                            err
                                                        ))
                                                    })
                                                    .unwrap();
                                                    vec![Token::Bytes(bytes)]
                                                }
                                                None => {
                                                    return get_error(
                                                        "No evm address specified for this domain"
                                                            .to_string(),
                                                    );
                                                }
                                            }
                                        }
                                        None => {
                                            // we will just query evm-address field
                                            match get_user_data(
                                                &provider,
                                                state.conf.contracts.starknetid,
                                                id,
                                                *EVM_ADDRESS,
                                            )
                                            .await
                                            {
                                                Some(addr) => {
                                                    let hex_addr = to_eth_hex(&addr);
                                                    let trimmed_hex_addr =
                                                        hex_addr.trim_start_matches("0x");
                                                    let bytes = ethers::utils::hex::decode(
                                                        trimmed_hex_addr,
                                                    )
                                                    .map_err(|err| {
                                                        get_error(format!(
                                                            "Invalid Structure: {}",
                                                            err
                                                        ))
                                                    })
                                                    .unwrap();
                                                    vec![Token::Bytes(bytes)]
                                                }
                                                None => {
                                                    return get_error(
                                                        "No evm address specified for this domain"
                                                            .to_string(),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                return get_error(format!("Chain not implemented: {}", chain));
                            }
                        }
                        ResolverFunctionCall::Addr(_bf) => {
                            match get_user_data_multicall(
                                &provider,
                                &state.conf,
                                id,
                                vec![*ETHEREUM, *EVM_ADDRESS],
                            )
                            .await
                            {
                                Some(addr) => {
                                    let eth_addr = to_eth_hex(&addr);
                                    let eth_addr = eth_addr
                                        .parse::<H160>()
                                        .map_err(|_| "Failed to parse address".to_string())
                                        .unwrap();
                                    vec![Token::Address(eth_addr)]
                                }
                                None => {
                                    return get_error(
                                        "No evm address specified for this domain".to_string(),
                                    );
                                }
                            }
                        }
                        _ => {
                            println!("Unimplemented Method");
                            Vec::new()
                        }
                    };

                    let ttl = 3600;
                    let expires = chrono::Utc::now().timestamp() as u64 + ttl;
                    let request_payload =
                        hex::decode(encoded_data.trim_start_matches("0x")).unwrap();
                    let data = ethers::abi::encode(&payload);
                    let request_hash = keccak256(request_payload).to_vec();
                    let result_hash = keccak256(&data).to_vec();

                    // Return signature
                    let wallet: LocalWallet =
                        LocalWallet::from_str(state.conf.evm.private_key.as_str()).unwrap();

                    match sign_message(wallet, &sender, expires, request_hash, result_hash, data) {
                        Ok(res) => (
                            StatusCode::OK,
                            Json(json!({
                                "data": res
                            })),
                        )
                            .into_response(),
                        Err(e) => get_error(format!("Error signing message : {}", e)),
                    }
                }
                Err(e) => get_error(format!("Error fetching identity : {}", e)),
            }
        }
        Err(e) => get_error(format!("Error decoding data: {:?}", e)),
    }
}
