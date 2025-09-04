use crate::{models::AppState, utils::to_hex};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet::core::types::FieldElement;
use std::sync::Arc;

const PAGE_SIZE: usize = 50;

#[derive(Deserialize)]
pub struct FetchNftsQuery {
    addr: FieldElement,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    data: Vec<ApiNftWrapper>,
}

#[derive(Debug, Deserialize)]
struct ApiNftWrapper {
    nft: ApiNft,
}

#[derive(Debug, Deserialize)]
struct ApiNft {
    #[serde(rename = "tokenId")]
    token_id: String,

    #[serde(rename = "collectionAddress")]
    collection_address: String,

    metadata: ApiMetadata,
}

#[derive(Debug, Deserialize)]
struct ApiMetadata {
    image: String,

    #[serde(rename = "imageType")]
    image_type: String,

    name: Option<String>,
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

#[derive(Serialize, Debug)]
pub struct Result {
    pub data: Vec<StarkscanNftProps>,
    pub next_url: Option<String>,
}

#[route(get, "/starkscan/fetch_nfts", crate::endpoints::starkscan::fetch_nfts)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FetchNftsQuery>,
) -> impl IntoResponse {
    // Parse page_index from query.cursor; default to 0
    let page_index_str = query.cursor.clone().unwrap_or_else(|| "0".to_string());
    let page_index: u32 = page_index_str.parse().unwrap_or(0);

    let addr_hex = to_hex(&query.addr);

    let url = format!(
        "{}/user/{}/collections?pageIndex={}&pageSize={}&isOnlyVerifiedOnes=true",
        state.conf.starkscan.api_url, addr_hex, page_index, PAGE_SIZE,
    );

    let client = reqwest::Client::new();

    // Try to fetch from Pyramid API with timeout
    let api_result = tokio::time::timeout(
        // 3 second timeout
        std::time::Duration::from_secs(3),
        client
            .get(&url)
            .header("accept", "application/json")
            .header("Token", "pyramid-alpha")
            .send(),
    )
    .await;

    match api_result {
        Ok(Ok(response)) => {
            // API call succeeded, try to parse response
            match response.text().await {
                Ok(text) => match serde_json::from_str::<ApiResponse>(&text) {
                    Ok(api_res) => {
                        // Convert API data to our StarkscanNftProps
                        let nfts: Vec<StarkscanNftProps> = api_res
                            .data
                            .into_iter()
                            .map(|item| {
                                let meta = item.nft.metadata;
                                let is_animation =
                                    meta.image_type.to_lowercase().starts_with("animation/");

                                StarkscanNftProps {
                                    animation_url: if is_animation {
                                        Some(meta.image.clone())
                                    } else {
                                        None
                                    },
                                    attributes: None,
                                    contract_address: item.nft.collection_address,
                                    description: None,
                                    external_url: None,
                                    image_url: Some(meta.image.clone()),
                                    image_medium_url: Some(meta.image.clone()),
                                    image_small_url: Some(meta.image.clone()),
                                    minted_at_transaction_hash: None,
                                    minted_by_address: None,
                                    token_id: item.nft.token_id.clone(),
                                    name: meta.name,
                                    nft_id: Some(item.nft.token_id.clone()),
                                    token_uri: None,
                                    minted_at_timestamp: 0,
                                }
                            })
                            .collect();

                        // Determine next_url
                        let next_url = if nfts.len() < PAGE_SIZE {
                            None
                        } else {
                            Some(format!(
                                "{}/starkscan/fetch_nfts?addr={}&cursor={}",
                                state.conf.server.base_url,
                                addr_hex,
                                page_index + 1
                            ))
                        };

                        // Return our custom struct
                        let result = Result {
                            data: nfts,
                            next_url,
                        };

                        (StatusCode::OK, Json(result)).into_response()
                    }
                    Err(e) => {
                        // Failed to parse API response, return fallback
                        state.logger.warning(format!("Failed to deserialize Pyramid API response: {} for response: {}. Returning fallback response.", e, text));
                        return_fallback_response(addr_hex, page_index, &state)
                    }
                },
                Err(e) => {
                    // Failed to get response text, return fallback
                    state.logger.warning(format!("Failed to get JSON response from Pyramid API: {}. Returning fallback response.", e));
                    return_fallback_response(addr_hex, page_index, &state)
                }
            }
        }
        Ok(Err(e)) => {
            // HTTP request failed, return fallback
            state.logger.warning(format!(
                "Pyramid API request failed: {}. Returning fallback response.",
                e
            ));
            return_fallback_response(addr_hex, page_index, &state)
        }
        Err(_) => {
            // Request timed out, return fallback
            state
                .logger
                .info("Pyramid API request timed out. Returning fallback response.".to_string());
            return_fallback_response(addr_hex, page_index, &state)
        }
    }
}

/// Fallback function to return a response when Pyramid API is unavailable
/// Returns an empty NFT collection with appropriate metadata
fn return_fallback_response(
    addr_hex: String,
    page_index: u32,
    state: &Arc<AppState>,
) -> axum::response::Response {
    state.logger.info(format!(
        "Returning fallback response for address: {} (page: {})",
        addr_hex, page_index
    ));

    // Return empty data but maintain the expected structure
    let fallback_result = Result {
        data: Vec::new(), // Empty NFT list
        next_url: None,   // No pagination since we have no data
    };

    // Add a custom header to indicate this is a fallback response
    let mut response = (StatusCode::OK, Json(fallback_result)).into_response();
    response
        .headers_mut()
        .insert("X-Fallback-Response", "true".parse().unwrap());
    response.headers_mut().insert(
        "X-Fallback-Reason",
        "pyramid-api-unavailable".parse().unwrap(),
    );

    response
}
