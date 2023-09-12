use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use futures::StreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc; // for stream handling

#[derive(Serialize)]
pub struct DomainData {
    domains: Vec<String>,
}

#[derive(Deserialize)]
pub struct DomainQuery {
    addr: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let subdomains = state.db.collection::<mongodb::bson::Document>("subdomains");
    let addr = &query.addr;
    let mut domains_list = Vec::new();

    let cursor = subdomains
        .find(
            doc! {
                "addr": addr,
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
                        let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                        domains_list.push(domain);
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
