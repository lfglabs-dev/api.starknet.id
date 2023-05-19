use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures::stream::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Serialize, Deserialize)]
pub struct FullId {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain_expiry: Option<i32>,
}

#[derive(Deserialize)]
pub struct AddrQuery {
    addr: String,
}

#[derive(Serialize)]
pub struct FullIdResponse {
    full_ids: Vec<FullId>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
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
                    let domain_expiry = doc.get_i32("domain_expiry").ok();
                    full_ids.push(FullId {
                        id,
                        domain,
                        domain_expiry,
                    });
                }
            }
            let response = FullIdResponse { full_ids };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
