use crate::{
    models::{AppState, IdentityData},
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::bson::{doc, from_bson, Bson, Document};
use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct IdQuery {
    id: FieldElement,
}

#[route(get, "/id_to_data", crate::endpoints::id_to_data)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let collection = state.starknetid_db.collection::<Document>("id_owners");

    let mut cursor = match collection
        .aggregate(get_pipeline(to_hex(&query.id)), None)
        .await
    {
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

fn get_pipeline(id: String) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "_cursor.to": null,
                "id": id
            }
        },
        doc! {
            "$lookup": {
                "from": "domains",
                "let": {"id": "$id"},
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                {"_cursor.to": null},
                                {"_cursor.to": {"$exists": false}}
                            ],
                            "$expr": {"$eq": ["$id", "$$id"]},
                        }
                    }
                ],
                "as": "domain_data"
            }
        },
        doc! {
            "$lookup": {
                "from": "id_user_data",
                "let": {"id": "$id"},
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                {"_cursor.to": null},
                                {"_cursor.to": {"$exists": false}}
                            ],
                            "$expr": {"$eq": ["$id", "$$id"]},
                            "data": { "$ne": null }
                        }
                    },
                    doc! {
                        "$project": {"_id": 0, "field": 1, "data": 1}
                    }
                ],
                "as": "user_data"
            }
        },
        doc! {
            "$lookup": {
                "from": "id_verifier_data",
                "let": {"id": "$id"},
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                {"_cursor.to": null},
                                {"_cursor.to": {"$exists": false}}
                            ],
                            "$expr": {"$eq": ["$id", "$$id"]},
                            "data": { "$ne": null }
                        }
                    },
                    doc! {
                        "$project": {"_id": 0, "field": 1, "data": 1, "verifier": 1}
                    }
                ],
                "as": "verifier_data"
            }
        },
        doc! {
            "$lookup": {
                "from": "id_verifier_data",
                "let": {"id": "$id"},
                "pipeline": [
                    doc! {
                        "$match": {
                            "$or": [
                                {"_cursor.to": null},
                                {"_cursor.to": {"$exists": false}}
                            ],
                            "$expr": {"$eq": ["$id", "$$id"]},
                            "extended_data": { "$ne": null }
                        }
                    },
                    doc! {
                        "$project": {"_id": 0, "field": 1, "extended_data": 1, "verifier": 1}
                    }
                ],
                "as": "extended_verifier_data"
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "owner": 1,
                "main": 1,
                "creation_date": 1,
                "domain": {
                    "domain": {"$arrayElemAt": ["$domain_data.domain", 0]},
                    "root": {"$arrayElemAt": ["$domain_data.root", 0]},
                    "migrated" : {"$arrayElemAt": ["$domain_data.migrated", 0]},
                    "creation_date": {"$arrayElemAt": ["$domain_data.creation_date", 0]},
                    "expiry": {"$arrayElemAt": ["$domain_data.expiry", 0]},
                    "resolver": {"$arrayElemAt": ["$domain_data.resolver", 0]},
                    "legacy_address": {"$arrayElemAt": ["$domain_data.legacy_address", 0]},
                    "rev_address": {"$arrayElemAt": ["$domain_data.rev_address", 0]}
                },
                "user_data": 1,
                "verifier_data": 1,
                "extended_verifier_data": 1
            }
        },
    ]
}
