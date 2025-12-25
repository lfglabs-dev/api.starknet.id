use mongodb::{
    bson::{from_bson, Bson},
    Database,
};
use starknet::core::types::Felt;

use crate::{
    config::{Config, OffchainResolver},
    logger::Logger,
    utils::to_hex,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct AppState {
    pub conf: Config,
    pub starknetid_db: Database,
    pub sales_db: Database,
    pub free_domains_db: Database,
    pub states: States,
    pub dynamic_offchain_resolvers: Arc<Mutex<HashMap<String, OffchainResolver>>>,
    pub logger: Logger,
}

fn serialize_felt<S>(field_element: &Felt, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let hex_string = to_hex(field_element);
    serializer.serialize_str(&hex_string)
}

fn serialize_opt_felt<S>(field_element: &Option<Felt>, serializer: S) -> Result<S::Ok, S::Error>
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

fn serialize_vec_felt<S>(vec: &Vec<Felt>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(vec.len()))?;
    for element in vec {
        seq.serialize_element(&SerializedFelt(element))?;
    }
    seq.end()
}

struct SerializedFelt<'a>(&'a Felt);

impl<'a> Serialize for SerializedFelt<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_felt(self.0, serializer)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IdentityData {
    #[serde(serialize_with = "serialize_felt")]
    pub id: Felt,
    #[serde(serialize_with = "serialize_felt")]
    pub owner: Felt,
    pub main: bool,
    pub creation_date: u64,
    #[serde(deserialize_with = "deserialize_optional_domain")]
    pub domain: Option<Domain>,
    pub user_data: Vec<UserData>,
    pub verifier_data: Vec<VerifierData>,
    pub extended_verifier_data: Vec<ExtendedVerifierData>,
}

fn deserialize_optional_domain<'de, D>(deserializer: D) -> Result<Option<Domain>, D::Error>
where
    D: Deserializer<'de>,
{
    let bson = Bson::deserialize(deserializer)?;
    match bson {
        Bson::Document(doc) if doc.is_empty() => Ok(None),
        Bson::Document(doc) => from_bson(Bson::Document(doc))
            .map(Some)
            .map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom(
            "expected a document for domain field",
        )),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Domain {
    pub domain: String,
    pub migrated: bool,
    pub root: bool,
    pub creation_date: u64,
    pub expiry: Option<u64>,
    #[serde(serialize_with = "serialize_opt_felt")]
    pub resolver: Option<Felt>,
    #[serde(serialize_with = "serialize_opt_felt")]
    pub legacy_address: Option<Felt>,
    #[serde(serialize_with = "serialize_opt_felt")]
    pub rev_address: Option<Felt>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserData {
    #[serde(serialize_with = "serialize_felt")]
    pub field: Felt,
    #[serde(serialize_with = "serialize_felt")]
    pub data: Felt,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VerifierData {
    #[serde(serialize_with = "serialize_felt")]
    pub verifier: Felt,
    #[serde(serialize_with = "serialize_felt")]
    pub field: Felt,
    #[serde(serialize_with = "serialize_felt")]
    pub data: Felt,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExtendedVerifierData {
    #[serde(serialize_with = "serialize_felt")]
    pub verifier: Felt,
    #[serde(serialize_with = "serialize_felt")]
    pub field: Felt,
    #[serde(serialize_with = "serialize_vec_felt")]
    pub extended_data: Vec<Felt>,
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

#[derive(Deserialize, Debug)]
pub struct OffchainResolverHint {
    pub address: Felt,
    pub r: Felt,
    pub s: Felt,
    pub max_validity: u64,
}
