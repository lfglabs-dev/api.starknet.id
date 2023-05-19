use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct Data {
    domain: String,
    addr: Option<String>,
    domain_expiry: Option<i32>,
    is_owner_main: bool,
}

#[derive(Deserialize)]
pub struct IdQuery {
    id: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let domains = state.db.collection::<mongodb::bson::Document>("domains");
    let starknet_ids = state
        .db
        .collection::<mongodb::bson::Document>("starknet_ids");

    let domain_document = domains
        .find_one(
            doc! {
                "token_id": &query.id,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    let domain_data = match domain_document {
        Ok(doc) => {
            if let Some(doc) = doc {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                let addr = doc.get_str("addr").ok().map(String::from);
                let expiry = doc.get_i32("expiry").ok();
                Some((domain, addr, expiry))
            } else {
                None
            }
        }
        Err(_) => return get_error("Error while fetching from database".to_string()),
    };

    let owner_document = starknet_ids
        .find_one(
            doc! {
                "token_id": &query.id,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    let owner = match owner_document {
        Ok(doc) => doc.and_then(|doc| doc.get_str("owner").ok().map(String::from)),
        Err(_) => return get_error("Error while fetching from database".to_string()),
    };

    if domain_data.is_none() || owner.is_none() {
        return get_error("no domain associated to this starknet id was found".to_string());
    }

    let (domain, addr, expiry) = domain_data.unwrap();
    let owner = owner.unwrap();

    let owner_document = domains
        .find_one(
            doc! {
                "domain": &domain,
                "addr": &owner,
                "rev_addr": &owner,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    let is_owner_main = owner_document.is_ok() && owner_document.unwrap().is_some();
    let data = Data {
        domain,
        addr,
        domain_expiry: expiry,
        is_owner_main,
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
