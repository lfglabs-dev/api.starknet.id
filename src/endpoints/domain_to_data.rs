use crate::{
    models::{AppState, IdentityData},
    utils::get_error,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::{doc, from_bson, Bson, Document};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct DomainQuery {
    domain: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let collection = state.starknetid_db.collection::<Document>("id_owners");

    let mut cursor = match collection.aggregate(get_pipeline(query.domain), None).await {
        Ok(cursor) => cursor,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                headers,
                "Failed to retrieve data".to_string(),
            )
                .into_response();
        }
    };

    // The aggregation returns a single document
    return if let Some(result) = cursor.next().await {
        match result {
            Ok(doc) => (
                StatusCode::OK,
                headers,
                Json(from_bson::<IdentityData>(Bson::Document(doc)).expect("Malformed document")),
            )
                .into_response(),
            Err(err) => get_error(format!("Unexpected error: {}", err)),
        }
    } else {
        get_error("Identity not found".to_string())
    };
}

fn get_pipeline(domain: String) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "_cursor.to": null,
                "domain": domain
            }
        },
        doc! {
            "$lookup": {
                "from": "id_owners",
                "let": {
                    "id": "$id"
                },
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                { "_cursor.to": null },
                                { "_cursor.to": { "$exists": false } }
                            ],
                            "$expr": { "$eq": ["$id", "$$id"] }
                        }
                    }
                ],
                "as": "id_data"
            }
        },
        doc! { "$unwind": "$id_data" },
        doc! {
            "$lookup": {
                "from": "id_user_data",
                "let": {
                    "id": "$id"
                },
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                { "_cursor.to": null },
                                { "_cursor.to": { "$exists": false } }
                            ],
                            "$expr": { "$eq": ["$id", "$$id"] }
                        }
                    },
                    doc! {
                        "$project": {
                            "_id": 0,
                            "field": 1,
                            "data": 1
                        }
                    }
                ],
                "as": "user_data"
            }
        },
        doc! {
            "$lookup": {
                "from": "id_verifier_data",
                "let": {
                    "id": "$id"
                },
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                { "_cursor.to": null },
                                { "_cursor.to": { "$exists": false } }
                            ],
                            "$expr": { "$eq": ["$id", "$$id"] }
                        }
                    },
                    doc! {
                        "$project": {
                            "_id": 0,
                            "field": 1,
                            "data": 1,
                            "verifier": 1
                        }
                    }
                ],
                "as": "verifier_data"
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "owner": "$id_data.owner",
                "main": "$id_data.main",
                "creation_date": "$id_data.creation_date",
                "domain": {
                    "domain": "$domain",
                    "root": "$root",
                    "creation_date": "$creation_date",
                    "expiry": "$expiry",
                    "resolver": "$resolver",
                    "legacy_address": "$legacy_address",
                    "rev_address": "$rev_address"
                },
                "user_data": 1,
                "verifier_data": 1
            }
        },
    ]
}
