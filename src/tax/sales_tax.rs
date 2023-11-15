use crate::models::States;
use std::{collections::HashMap, fs};

pub async fn load_sales_tax() -> States {
    match fs::read_to_string("./src/tax/sales_tax.json") {
        Ok(data) => match serde_json::from_str(&data) {
            Ok(states) => states,
            Err(e) => {
                println!("Unable to parse sales tax file: {}", e);
                States {
                    states: HashMap::new(),
                }
            }
        },
        Err(e) => {
            println!("Unable to load sales tax file: {}", e);
            States {
                states: HashMap::new(),
            }
        }
    }
}
