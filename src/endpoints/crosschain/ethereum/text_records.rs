use anyhow::{Context, Result, anyhow};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::{cairo_short_string_to_felt, parse_cairo_short_string},
    },
    macros::selector,
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};

use crate::config::{Config, EvmRecordVerifier};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum HandlerType {
    GetDiscordName,
    GetGithubName,
    GetTwitterName,
}

#[derive(Deserialize, Debug)]
struct GithubUser {
    login: String,
}

#[derive(Deserialize, Debug)]
struct DiscordUser {
    username: String,
}

impl EvmRecordVerifier {
    pub async fn execute_handler(&self, config: &Config, id: FieldElement) -> Result<String> {
        match self.handler {
            HandlerType::GetDiscordName => self.get_discord_name(config, id).await,
            HandlerType::GetGithubName => self.get_github_name(id).await,
            HandlerType::GetTwitterName => self.get_twitter_name(config, id).await,
        }
    }

    async fn get_discord_name(&self, config: &Config, id: FieldElement) -> Result<String> {
        let social_id = FieldElement::to_string(&id);
        let url = format!("https://discord.com/api/users/{}", social_id);
        let client = Client::new();
        let resp = client
            .get(&url)
            .header("Content-Type", "application/json")
            .header(
                "Authorization",
                format!("Bot {}", config.variables.discord_token),
            )
            .send()
            .await?
            .json::<DiscordUser>()
            .await
            .context("Failed to parse JSON response from Discord API")?;

        Ok(resp.username)
    }
    async fn get_github_name(&self, id: FieldElement) -> Result<String> {
        let social_id = FieldElement::to_string(&id);
        let url = format!("https://api.github.com/user/{}", social_id);
        let client = Client::builder()
            .user_agent("request")
            .build()
            .context("Failed to build HTTP client")?;
        let response = client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to GitHub")?;

        if response.status() != StatusCode::OK {
            anyhow::bail!("GitHub API returned non-OK status: {}", response.status());
        }

        let text = response
            .text()
            .await
            .context("Failed to read response text")?;
        let user: GithubUser =
            serde_json::from_str(&text).context("Failed to deserialize GitHub response")?;

        Ok(user.login)
    }

    async fn get_twitter_name(&self, config: &Config, id: FieldElement) -> Result<String> {
        let social_id = FieldElement::to_string(&id);
        let client = Client::new();
        let response = client.get("https://twttrapi.p.rapidapi.com/get-user-by-id")
            .header("X-RapidAPI-Key", config.variables.twitter_api_key.clone())
            .header("X-RapidAPI-Host", "twttrapi.p.rapidapi.com")
            .query(&[("user_id", &social_id)])
            .send()
            .await?;

        if response.status() != StatusCode::OK {
            anyhow::bail!("Twitter API returned non-OK status: {}", response.status());
        }
        let response_body = response.text().await?;
        let json: Value = serde_json::from_str(&response_body)?;
        let screen_name = json
            .get("data")
            .and_then(|data| data.get("user_result"))
            .and_then(|user_result| user_result.get("result"))
            .and_then(|result| result.get("legacy"))
            .and_then(|legacy| legacy.get("screen_name"))
            .and_then(|screen_name| screen_name.as_str())
            .ok_or_else(|| anyhow!("Failed to extract screen name"));

        Ok(screen_name.map(|name| name.to_string()).unwrap())
    }
}

pub async fn get_verifier_data(
    config: &Config,
    provider: &JsonRpcClient<HttpTransport>,
    id: FieldElement,
    record_config: &EvmRecordVerifier,
) -> Option<String> {
    let call_result = provider
        .call(
            FunctionCall {
                contract_address: config.contracts.starknetid,
                entry_point_selector: selector!("get_verifier_data"),
                calldata: vec![
                    id,
                    cairo_short_string_to_felt(&record_config.field).unwrap(),
                    record_config.verifier_contract,
                    FieldElement::ZERO,
                ],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await;

    match call_result {
        Ok(result) => {
            if result[0] == FieldElement::ZERO {
                return None;
            }

            match record_config.execute_handler(config, result[0]).await {
                Ok(name) => Some(name),
                Err(e) => {
                    println!("Error while executing handler: {:?}", e);
                    None
                }
            }
        }
        Err(e) => {
            println!("Error while fetchingverifier data: {:?}", e);
            None
        }
    }
}

pub async fn get_unbounded_user_data(
    config: &Config,
    provider: &JsonRpcClient<HttpTransport>,
    id: FieldElement,
    field: &str,
) -> Option<String> {
    let call_result = provider
        .call(
            FunctionCall {
                contract_address: config.contracts.starknetid,
                entry_point_selector: selector!("get_unbounded_user_data"),
                calldata: vec![
                    id,
                    cairo_short_string_to_felt(field).unwrap(),
                    FieldElement::ZERO,
                ],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await;
    match call_result {
        Ok(result) => {
            if result[0] == FieldElement::ZERO {
                return None;
            }
            let res = result
                .iter()
                .skip(1)
                .filter_map(|val| parse_cairo_short_string(val).ok())
                .collect::<Vec<String>>() // Collect into a vector of strings
                .join("");
            Some(res)
        }
        Err(e) => {
            println!("Error while fetchingverifier data: {:?}", e);
            None
        }
    }
}
