use std::{str::FromStr, sync::Arc};

use crate::{
    endpoints::crosschain::ethereum::utils::{decode_data, sign_message, to_eth_hex},
    models::AppState,
    utils::get_error,
};
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
use mongodb::bson::doc;
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
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
        Ok(name) => {
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
            calldata.extend(encoded_domain);
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

                    // get the fields to query
                    let fields = if let Some(field) =
                        state.conf.reversed_evm_resolvers.get(sender.as_str())
                    {
                        vec![field, "evm-address"]
                    } else {
                        vec!["evm-address"]
                    };

                    // for each field query user data
                    for field in fields {
                        let call_result = provider
                            .call(
                                FunctionCall {
                                    contract_address: state.conf.contracts.starknetid,
                                    entry_point_selector: selector!("get_user_data"),
                                    calldata: vec![
                                        id,
                                        cairo_short_string_to_felt(field).unwrap(),
                                        FieldElement::ZERO,
                                    ],
                                },
                                BlockId::Tag(BlockTag::Latest),
                            )
                            .await;
                        match call_result {
                            Ok(result) => {
                                if result[0] != FieldElement::ZERO {
                                    let hex_addr = to_eth_hex(&result[0]);
                                    let payload: Vec<Token> = vec![Token::Address(
                                        hex_addr.parse::<H160>().expect("Failed to parse address"),
                                    )];

                                    let ttl = 3600;
                                    let expires = chrono::Utc::now().timestamp() as u64 + ttl;
                                    let request_payload =
                                        hex::decode(encoded_data.trim_start_matches("0x")).unwrap();
                                    let data = ethers::abi::encode(&payload);
                                    let request_hash = keccak256(request_payload).to_vec();
                                    let result_hash = keccak256(&data).to_vec();

                                    // Return signature
                                    let wallet: LocalWallet =
                                        LocalWallet::from_str(state.conf.evm.private_key.as_str())
                                            .unwrap();

                                    match sign_message(
                                        wallet,
                                        &sender,
                                        expires,
                                        request_hash,
                                        result_hash,
                                        data,
                                    ) {
                                        Ok(res) => {
                                            return (
                                                StatusCode::OK,
                                                Json(json!({
                                                    "data": res
                                                })),
                                            )
                                                .into_response();
                                        }
                                        Err(e) => {
                                            return get_error(format!(
                                                "Error signing message : {}",
                                                e
                                            ));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Error fetching user data for field {} : {}", field, e)
                            }
                        }
                    }
                    get_error("No evm address specified for this domain".to_string())
                }
                Err(e) => get_error(format!("Error fetching identity : {}", e)),
            }
        }
        Err(e) => get_error(format!("Error decoding data: {:?}", e)),
    }
}
