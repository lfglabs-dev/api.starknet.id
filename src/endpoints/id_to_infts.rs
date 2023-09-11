use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct IdentityNFT {
    contract: String,
    inft_id: String,
}

#[derive(Deserialize)]
pub struct NFTQuery {
    id: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<NFTQuery>,
) -> impl IntoResponse {
    let infts = state
        .db
        .collection::<mongodb::bson::Document>("equipped_infts");
    let documents = infts
        .find(
            doc! {
                "starknet_id": &query.id,
                "_cursor.to": null,
            },
            None,
        )
        .await;

    match documents {
        Ok(mut cursor) => {
            let mut output = Vec::<IdentityNFT>::new();

            while let Some(doc_result) = cursor.next().await {
                match doc_result {
                    Ok(doc) => {
                        let contract = doc.get_str("contract").unwrap_or_default().to_owned();
                        let inft_id = doc.get_str("inft_id").unwrap_or_default().to_owned();
                        let nft = IdentityNFT { contract, inft_id };
                        output.push(nft);
                    }
                    Err(_) => return get_error("Error reading database document".to_string()),
                }
            }

            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("max-age=30"),
            );

            (StatusCode::OK, headers, Json(output)).into_response()
        }
        Err(_) => get_error("Error fetching from database".to_string()),
    }
}
