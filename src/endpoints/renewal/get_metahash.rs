use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::TryStreamExt;
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct GetMetaHashData {
    meta_hash: String,
    tax_rate: f32,
}

#[derive(Deserialize)]
pub struct GetMetaHashQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetMetaHashQuery>,
) -> impl IntoResponse {
    let sales_collection = state
        .sales_db
        .collection::<mongodb::bson::Document>("sales");

    let pipeline = vec![
        doc! {"$match": {
            "payer": to_hex(&query.addr),
            "$or": [
                { "_cursor.to": { "$exists": false } },
                { "_cursor.to": Bson::Null },
            ],
        }},
        doc! {"$sort": {"timestamp": -1}}, // take the most recent entry
        doc! {"$lookup": {
            "from": "metadata",
            "localField": "meta_hash",
            "foreignField": "meta_hash",
            "as": "metadata_info"
        }},
        doc! {"$unwind": {
            "path": "$metadata_info",
            "preserveNullAndEmptyArrays": true
        }},
        doc! {"$project": {
            "meta_hash": 1,
            "tax_state": {
                "$ifNull": ["$metadata_info.tax_state", ""]
            }
        }},
        doc! {"$limit": 1},
    ];

    let cursor = sales_collection.aggregate(pipeline, None).await;
    match cursor {
        Ok(cursor) => match cursor.try_collect::<Vec<mongodb::bson::Document>>().await {
            Ok(documents) => {
                if documents.is_empty() {
                    return get_error("No documents found".to_string());
                }

                let mut headers = HeaderMap::new();
                headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

                println!("docs: {:?}", documents);
                for doc in documents {
                    if let Ok(meta_hash) = doc.get_str("meta_hash") {
                        let mut tax_rate = 0.0;
                        if let Some(tax_state) = doc.get("tax_state").and_then(|ts| ts.as_str()) {
                            if let Some(state_info) =
                                state.states.states.get(&tax_state.to_string())
                            {
                                tax_rate = state_info.rate;
                            }
                        }
                        let res = GetMetaHashData {
                            meta_hash: meta_hash.to_string(),
                            tax_rate,
                        };
                        return (StatusCode::OK, headers, Json(res)).into_response();
                    }
                }
                get_error("No metahash found".to_string())
            }
            Err(_) => get_error("Error while fetching from database".to_string()),
        },
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
