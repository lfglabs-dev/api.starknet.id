use crate::{
    models::AppState,
    resolving::get_custom_resolver,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures::StreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct AvailableIds {
    ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct AddrQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    let starknet_ids = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");
    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let addr = to_hex(&query.addr);
    let documents = starknet_ids
        .find(
            doc! {
                "owner": &addr,
                "id" : {
                    "$ne" : null
                  },
                "_cursor.to": null,
            },
            None,
        )
        .await;

    let mut ids: Vec<String> = Vec::new();

    match documents {
        Ok(mut cursor) => {
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let token_id = doc.get_str("id").unwrap_or_default().to_owned();
                    let domain_doc = domains
                        .find_one(
                            doc! {
                                "id": &token_id,
                                "_cursor.to": null,
                            },
                            None,
                        )
                        .await;

                    if let Ok(doc_opt) = domain_doc {
                        if let Some(doc) = doc_opt {
                            let domain_rs = doc.get_str("domain");
                            if let Ok(domain) = domain_rs {
                                if get_custom_resolver(&domains, domain).await.is_none() {
                                    continue;
                                }
                            }
                        }
                        ids.push(FieldElement::from_hex_be(&token_id).unwrap().to_string());
                    }
                }
            }
            (StatusCode::OK, Json(AvailableIds { ids })).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
