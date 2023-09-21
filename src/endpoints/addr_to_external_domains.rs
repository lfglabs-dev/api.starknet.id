use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use futures::StreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc; // for stream handling

#[derive(Serialize)]
pub struct DomainData {
    domains: Vec<String>,
}

#[derive(Deserialize)]
pub struct DomainQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let subdomains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("custom_resolutions");
    let addr = &query.addr;
    let mut domains_list = Vec::new();

    let cursor = subdomains
        .find(
            doc! {
                "field" : "starknet",
                "value": to_hex(addr),
                "_cursor.to": null,
            },
            None,
        )
        .await;

    match cursor {
        Ok(mut cursor) => {
            while let Some(result) = cursor.next().await {
                match result {
                    Ok(doc) => {
                        let domain_slice =
                            doc.get_str("domain_slice").unwrap_or_default().to_owned();
                        let resolver =
                            FieldElement::from_hex_be(doc.get_str("resolver").unwrap_or_default())
                                .unwrap();
                        match state.conf.custom_resolvers.get(&to_hex(&resolver)) {
                            // a resolver can be associated to multiple domains, eg: argent.stark and ag.stark
                            Some(parents) => {
                                parents.iter().for_each(|parent| {
                                    // we automatically add all domains
                                    domains_list.push(format!("{}{}", domain_slice, parent));
                                });
                            }
                            None => {}
                        }
                    }
                    Err(_) => return get_error("Error while fetching from database".to_string()),
                }
            }

            // setting cache-control headers
            let mut headers = HeaderMap::new();
            headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

            let response = axum::response::Json(DomainData {
                domains: domains_list,
            });
            (StatusCode::OK, headers, response).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
