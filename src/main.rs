#![recursion_limit = "256"]

mod config;
mod ecdsa_sign;
mod endpoints;
mod logger;
mod models;
mod resolving;
mod tax;
mod utils;

use axum::{http::StatusCode, Router};
use axum_auto_routes::route;
use mongodb::{bson::doc, options::ClientOptions, Client};
use std::collections::HashMap;
use std::sync::Arc;
use std::{net::SocketAddr, sync::Mutex};
use tokio::time::{sleep, Duration};
use utils::WithState;

use tower_http::cors::{Any, CorsLayer};

use crate::resolving::update_offchain_resolvers;

lazy_static::lazy_static! {
    pub static ref ROUTE_REGISTRY: Mutex<Vec<Box<dyn WithState>>> = Mutex::new(Vec::new());
}

#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() {
    println!("starknetid_server: starting v{}", env!("CARGO_PKG_VERSION"));
    let conf = config::load();

    let logger = logger::Logger::new(&conf.watchtower);

    // Testing logger when server started
    logger.info(format!(
        "id_server: starting v{}",
        env!("CARGO_PKG_VERSION")
    ));
    logger.warning(format!(
        "id_server: starting v{}",
        env!("CARGO_PKG_VERSION")
    ));
    logger.severe(format!(
        "id_server: starting v{}",
        env!("CARGO_PKG_VERSION")
    ));

    let starknetid_client_options =
        ClientOptions::parse(&conf.databases.starknetid.connection_string)
            .await
            .unwrap();

    let sales_client_options = ClientOptions::parse(&conf.databases.sales.connection_string)
        .await
        .unwrap();
    let free_domains_client_options =
        ClientOptions::parse(&conf.databases.free_domains.connection_string)
            .await
            .unwrap();

    let states = tax::sales_tax::load_sales_tax().await;
    if states.states.is_empty() {
        println!("error: unable to load sales tax");
        return;
    }

    let shared_state = Arc::new(models::AppState {
        conf: conf.clone(),
        starknetid_db: Client::with_options(starknetid_client_options)
            .unwrap()
            .database(&conf.databases.starknetid.name),
        sales_db: Client::with_options(sales_client_options)
            .unwrap()
            .database(&conf.databases.sales.name),
        free_domains_db: Client::with_options(free_domains_client_options)
            .unwrap()
            .database(&conf.databases.free_domains.name),
        states,
        dynamic_offchain_resolvers: Arc::new(Mutex::new(HashMap::new())),
    });
    // we will know by looking at the log number which db has an issue
    for db in [&shared_state.starknetid_db, &shared_state.sales_db] {
        if db.run_command(doc! {"ping": 1}, None).await.is_err() {
            println!("error: unable to connect to a database");
            return;
        } else {
            println!("database: connected")
        }
    }

    // refresh offchain resolvers from indexed data
    let refresh_state = shared_state.clone();
    tokio::spawn(async move {
        loop {
            update_offchain_resolvers(&refresh_state).await;
            sleep(Duration::from_millis(
                (conf.variables.refresh_delay * 1000.0) as u64,
            ))
            .await;
        }
    });

    let cors = CorsLayer::new().allow_headers(Any).allow_origin(Any);
    let app = ROUTE_REGISTRY
        .lock()
        .unwrap()
        .clone()
        .into_iter()
        .fold(Router::new().with_state(shared_state.clone()), |acc, r| {
            acc.merge(r.to_router(shared_state.clone()))
        })
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], conf.server.port));
    println!("server: listening on http://0.0.0.0:{}", conf.server.port);
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

#[route(get, "/")]
async fn root() -> (StatusCode, String) {
    (
        StatusCode::ACCEPTED,
        format!("starknetid_server v{}", env!("CARGO_PKG_VERSION")),
    )
}
