use crate::{logger::Logger , config ,models::States};
use std::{collections::HashMap, fs};

pub async fn load_sales_tax() -> States {
    let conf = config::load();
    let logger = Logger::new(&conf.watchtower);
    match fs::read_to_string("./src/tax/sales_tax.json") {
        Ok(data) => match serde_json::from_str(&data) {
            Ok(states) => states,
            Err(e) => {
                logger.warning(format!("Unable to parse sales tax file: {}", e));
                States {
                    states: HashMap::new(),
                }
            }
        },
        Err(e) => {
            logger.severe(format!("Unable to load sales tax file: {}", e));
            States {
                states: HashMap::new(),
            }
        }
    }
}
