use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_auto_routes::route;
use chrono::Duration;
use serde::Deserialize;
use serde_json::json;
use starknet::core::{crypto::{ecdsa_sign, pedersen_hash}, types::FieldElement};

use crate::{models::AppState, utils::get_error};

#[derive(Deserialize)]
pub struct AddrQuery {
    erc20_addr: FieldElement,
}

#[derive(Deserialize, Debug)]
pub struct AvnuApiResult {
    address: FieldElement,
    currentPrice: f64,
}

lazy_static::lazy_static! {
    static ref QUOTE_STR: FieldElement = FieldElement::from_dec_str("724720344857006587549020016926517802128122613457935427138661").unwrap();
}

#[route(get, "/get_altcoin_quote", crate::endpoints::avnu::get_altcoin_quote)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    // check if erc20_addr is whitelisted
    if !state
        .conf
        .token_support
        .whitelisted_tokens
        .contains(&query.erc20_addr)
    {
        return get_error("Token not supported".to_string());
    }

    // fetch quote from avnu api
    let url = format!(
        "{}/tokens/short?in=0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        state.conf.token_support.avnu_api
    );
    let client = reqwest::Client::new();
    match client.get(&url).send().await {
        Ok(response) => match response.text().await {
            Ok(text) => match serde_json::from_str::<Vec<AvnuApiResult>>(&text) {
                Ok(res) => {
                    let result = res
                        .iter()
                        .find(|&api_response| api_response.address == query.erc20_addr);
                    match result {
                        Some(data) => {
                            // compute message hash
                            let now = chrono::Utc::now();
                            let max_validity_timestamp = (now
                                + Duration::seconds(state.conf.token_support.max_validity))
                            .timestamp();
                            let quote = 1.0 / data.currentPrice;
                            // convert current price to wei and return an integer as AVNU api can use more than 18 decimals
                            let current_price_wei = (quote * (10u128.pow(18) as f64)).to_string();
                            let message_hash = pedersen_hash(
                                &pedersen_hash(
                                    &pedersen_hash(
                                        &query.erc20_addr,
                                        &FieldElement::from_dec_str(
                                            current_price_wei.to_string().as_str(),
                                        )
                                        .unwrap(),
                                    ),
                                    &FieldElement::from_dec_str(max_validity_timestamp.to_string().as_str()).unwrap(),
                                ),
                                &QUOTE_STR,
                            );
                            match ecdsa_sign(&state.conf.token_support.private_key.clone(), &message_hash) {
                                Ok(signature) => (StatusCode::OK, Json(json!({
                                    "quote": current_price_wei,
                                    "r": signature.r,
                                    "s": signature.s,
                                    "max_validity": max_validity_timestamp
                                }))).into_response(),
                                Err(e) => get_error(format!(
                                    "Error while generating Starknet signature: {}",
                                    e
                                )),
                            }
                        },
                        None => get_error("Token address not found".to_string()),
                    }
                }
                Err(e) => get_error(format!(
                    "Failed to deserialize result from AVNU API: {} for response: {}",
                    e, text
                )),
            },
            Err(e) => get_error(format!(
                "Failed to get JSON response while fetching token quote: {}",
                e
            )),
        },
        Err(e) => get_error(format!("Failed to fetch quote from AVNU api: {}", e)),
    }
}
