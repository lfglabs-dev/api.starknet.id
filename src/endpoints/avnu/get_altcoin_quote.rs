use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use starknet::core::types::FieldElement;

use crate::{models::AppState, utils::get_error};

#[derive(Deserialize)]
pub struct AddrQuery {
    erc20_addr: FieldElement,
}

#[derive(Deserialize, Debug)]
pub struct AvnuApiResult {
    address: FieldElement,
    #[allow(non_snake_case)]
    currentPrice: f64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    // check is erc20_addr is whitelist
    if !state.conf.token_support.whitelisted_tokens.contains(&query.erc20_addr) {
        return get_error("Token not supported".to_string());
    }

    let url = format!(
        "{}/tokens/short?in=0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        state.conf.token_support.avnu_api
    );
    let client = reqwest::Client::new();
    match client
        .get(&url)
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(text) => match serde_json::from_str::<Vec<AvnuApiResult>>(&text) {
                Ok(res) => {
                    let result = res.iter().find(|&api_response| api_response.address == query.erc20_addr);
                    println!("result: {:?}", result);
                    match result {
                        Some(data) => (StatusCode::OK, Json(data.currentPrice)).into_response(),
                        None => get_error("Token address not found".to_string()),
                    }
                },
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