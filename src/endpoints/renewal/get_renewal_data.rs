use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use futures::TryStreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct StarknetIdData {
    starknet_id: String,
}

#[derive(Deserialize)]
pub struct StarknetIdQuery {
    addr: FieldElement,
    domain: String,
}

#[route(
    get,
    "/renewal/get_renewal_data",
    crate::endpoints::renewal::get_renewal_data
)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StarknetIdQuery>,
) -> impl IntoResponse {
    // Fetch data from both collections and combine the results
    let auto_renew_flows_future = find_renewal_data(&state, "auto_renew_flows", &query);
    let auto_renew_flows_altcoins_future =
        find_renewal_data(&state, "auto_renew_flows_altcoins", &query);

    let (auto_renew_flows, auto_renew_flows_altcoins) =
        futures::join!(auto_renew_flows_future, auto_renew_flows_altcoins_future);

    let mut combined_results = Vec::new();

    if let Ok(results) = auto_renew_flows {
        combined_results.extend(results);
    }

    if let Ok(results) = auto_renew_flows_altcoins {
        combined_results.extend(results);
    }

    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    if !combined_results.is_empty() {
        for doc in &mut combined_results {
            doc.remove("_id");
            doc.remove("_cursor");
        }
        (StatusCode::OK, headers, Json(combined_results)).into_response()
    } else {
        get_error("Error while fetching from database or no results found".to_string())
    }
}

async fn find_renewal_data(
    state: &AppState,
    collection_name: &str,
    query: &StarknetIdQuery,
) -> mongodb::error::Result<Vec<mongodb::bson::Document>> {
    let collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>(collection_name);

    let mut cursor = collection
        .find(
            doc! {
                "renewer_address": to_hex(&query.addr),
                "domain": &query.domain,
                "$or": [
                    { "_cursor.to": { "$exists": false } },
                    { "_cursor.to": null },
                ],
            },
            None,
        )
        .await?;

    let mut documents = Vec::new();
    while let Some(result) = cursor.try_next().await? {
        documents.push(result);
    }

    Ok(documents)
}
