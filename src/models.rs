use mongodb::Database;
use starknet::core::types::FieldElement;

use crate::{config::Config, utils::to_hex};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;

pub struct AppState {
    pub conf: Config,
    pub starknetid_db: Database,
    pub sales_db: Database,
    pub states: States,
}

fn serialize_felt<S>(field_element: &FieldElement, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let hex_string = to_hex(field_element);
    serializer.serialize_str(&hex_string)
}

fn serialize_opt_felt<S>(
    field_element: &Option<FieldElement>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match field_element {
        Some(fe) => {
            let hex_string = to_hex(fe);
            serializer.serialize_str(&hex_string)
        }
        None => serializer.serialize_none(),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IdentityData {
    #[serde(serialize_with = "serialize_felt")]
    pub id: FieldElement,
    #[serde(serialize_with = "serialize_felt")]
    pub owner: FieldElement,
    pub main: bool,
    pub creation_date: u64,
    pub domain: Option<Domain>,
    pub user_data: Vec<UserData>,
    pub verifier_data: Vec<VerifierData>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Domain {
    pub domain: String,
    pub migrated: bool,
    pub root: bool,
    pub creation_date: u64,
    pub expiry: Option<u64>,
    #[serde(serialize_with = "serialize_opt_felt")]
    pub resolver: Option<FieldElement>,
    #[serde(serialize_with = "serialize_opt_felt")]
    pub legacy_address: Option<FieldElement>,
    #[serde(serialize_with = "serialize_opt_felt")]
    pub rev_address: Option<FieldElement>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserData {
    #[serde(serialize_with = "serialize_felt")]
    pub field: FieldElement,
    #[serde(serialize_with = "serialize_felt")]
    pub data: FieldElement,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VerifierData {
    #[serde(serialize_with = "serialize_felt")]
    pub verifier: FieldElement,
    #[serde(serialize_with = "serialize_felt")]
    pub field: FieldElement,
    #[serde(serialize_with = "serialize_felt")]
    pub data: FieldElement,
}

#[derive(Deserialize, Debug)]
pub struct State {
    pub rate: f32,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Deserialize, Debug)]
pub struct States {
    pub states: HashMap<String, State>,
}
