use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use futures::stream::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct FullId {
    id: String,
    domain: Option<String>,
    domain_expiry: Option<String>,
}

#[derive(Deserialize)]
pub struct AddrQuery {
    addr: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<AddrQuery>,
) -> impl IntoResponse {
    let starknet_ids = state
        .db
        .collection::<mongodb::bson::Document>("starknet_ids");

    let pipeline = vec![
        doc! {
            "$match": {
                "owner": &query.addr,
                "_chain.valid_to": null,
            },
        },
        doc! {
            "$lookup": {
                "from": "domains",
                "let": { "token_id": "$token_id" },
                "pipeline": [
                    {
                        "$match": {
                            "$expr": {
                                "$and": [
                                    { "$eq": ["$token_id", "$$token_id"] },
                                    { "$eq": ["$_chain.valid_to", null] },
                                ],
                            },
                        },
                    },
                ],
                "as": "domainData",
            },
        },
        doc! {
            "$unwind": {
                "path": "$domainData",
                "preserveNullAndEmptyArrays": true,
            },
        },
        doc! {
            "$project": {
                "_id": 0,
                "id": "$token_id",
                "domain": "$domainData.domain",
                "domain_expiry": "$domainData.expiry",
            },
        },
    ];

    let aggregate_options = AggregateOptions::default();
    let cursor = starknet_ids.aggregate(pipeline, aggregate_options).await;

    match cursor {
        Ok(mut cursor) => {
            let mut full_ids = Vec::new();
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let id = doc.get_str("id").unwrap_or_default().to_owned();
                    let domain = doc.get_str("domain").ok().map(String::from);
                    let domain_expiry = doc.get_str("domain_expiry").ok().map(String::from);

                    full_ids.push(FullId {
                        id,
                        domain,
                        domain_expiry,
                    });
                }
            }

            (StatusCode::OK, Json(full_ids)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
