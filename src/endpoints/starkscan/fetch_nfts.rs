use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct FetchNftsQuery {
    addr: FieldElement,
    cursor: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StarkscanApiResult {
    data: Vec<StarkscanNftProps>,
    next_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StarkscanNftProps {
    animation_url: Option<String>,
    attributes: Option<Value>,
    contract_address: String,
    description: Option<String>,
    external_url: Option<String>,
    image_url: Option<String>,
    image_medium_url: Option<String>,
    image_small_url: Option<String>,
    minted_at_transaction_hash: Option<String>,
    minted_by_address: Option<String>,
    token_id: String,
    name: Option<String>,
    nft_id: Option<String>,
    token_uri: Option<String>,
    minted_at_timestamp: i64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FetchNftsQuery>,
) -> impl IntoResponse {
    let base_url = format!(
        "{}nfts?owner_address={}",
        state.conf.starkscan.api_url,
        to_hex(&query.addr)
    );
    let url = query.cursor.as_ref().map_or(base_url.clone(), |cursor| {
        format!("{}&cursor={}", base_url, cursor)
    });

    let client = reqwest::Client::new();
    match client
        .get(&url)
        .header("accept", "application/json")
        .header("x-api-key", state.conf.starkscan.api_key.clone())
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(text) => match serde_json::from_str::<StarkscanApiResult>(&text) {
                Ok(res) => (StatusCode::OK, Json(res)).into_response(),
                Err(e) => get_error(format!(
                    "Failed to deserialize result from Starkscan API: {} for response: {}",
                    e, text
                )),
            },
            Err(e) => get_error(format!(
                "Failed to get JSON response while fetching user NFT data: {}",
                e
            )),
        },
        Err(e) => get_error(format!("Failed to fetch user NFTs from API: {}", e)),
    }
}
