use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures::stream::StreamExt;
use mongodb::{
    bson::{doc, Bson},
    options::AggregateOptions,
};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
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
    addr: FieldElement,
}

#[derive(Serialize)]
pub struct FullIdResponse {
    full_ids: Vec<FullId>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    let id_owners = state.db.collection::<mongodb::bson::Document>("id_owners");

    let pipeline = vec![
        doc! {
            "$match": {
                "owner": to_hex(&query.addr),
                "_cursor.to": Bson::Null
            },
        },
        doc! {
            "$lookup": {
                "from": "domains",
                "let": { "local_id": "$id" },
                "pipeline": [
                    { "$match": {
                        "$expr": { "$eq": [ "$id", "$$local_id" ] },
                        "_cursor.to": Bson::Null
                    }}
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
                "id": 1,
                "domain": "$domainData.domain",
                "domain_expiry": "$domainData.expiry",
            },
        },
    ];

    let aggregate_options = AggregateOptions::default();
    let cursor = id_owners.aggregate(pipeline, aggregate_options).await;

    match cursor {
        Ok(mut cursor) => {
            let mut full_ids = Vec::new();
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let id = FieldElement::from_hex_be(
                        &doc.get_str("id").unwrap_or_default().to_owned(),
                    )
                    .unwrap()
                    .to_string();
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
