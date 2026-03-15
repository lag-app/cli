// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::api::ApiClient;
use crate::auth;
use crate::cli::FriendsAction;
use anyhow::Result;

pub async fn run(action: Option<FriendsAction>) -> Result<()> {
    let creds = auth::require_auth()?;
    let mut api = ApiClient::new(creds)?;

    match action {
        None => list_friends(&mut api).await,
        Some(FriendsAction::Add { username }) => add_friend(&mut api, &username).await,
        Some(FriendsAction::Remove { username }) => remove_friend(&mut api, &username).await,
        Some(FriendsAction::Requests) => show_requests(&mut api).await,
        Some(FriendsAction::Accept { username }) => accept_request(&mut api, &username).await,
        Some(FriendsAction::Decline { username }) => decline_request(&mut api, &username).await,
    }
}

async fn list_friends(api: &mut ApiClient) -> Result<()> {
    let friends: Vec<serde_json::Value> = api.get("/friends").await?;

    if friends.is_empty() {
        println!("No friends yet. Add one with `lag friends add <username>`.");
        return Ok(());
    }

    println!("Friends:\n");
    for friend in &friends {
        let username = friend["user"]["displayName"]
            .as_str()
            .or_else(|| friend["user"]["username"].as_str())
            .or_else(|| friend["username"].as_str())
            .unwrap_or("?");
        let status = friend["user"]["status"]
            .as_str()
            .or_else(|| friend["status"].as_str())
            .unwrap_or("offline");
        let indicator = match status {
            "online" => "+",
            "idle" => "~",
            _ => "-",
        };
        println!("  {} {} ({})", indicator, username, status);
    }

    Ok(())
}

async fn add_friend(api: &mut ApiClient, username: &str) -> Result<()> {
    let _: serde_json::Value = api
        .post(
            "/friends/request",
            &serde_json::json!({ "username": username }),
        )
        .await?;
    println!("Friend request sent to {}.", username);
    Ok(())
}

async fn remove_friend(api: &mut ApiClient, username: &str) -> Result<()> {
    let friends: Vec<serde_json::Value> = api.get("/friends").await?;
    let friend = friends
        .iter()
        .find(|f| {
            f["user"]["username"].as_str() == Some(username)
                || f["username"].as_str() == Some(username)
        })
        .ok_or_else(|| anyhow::anyhow!("Friend '{}' not found", username))?;

    let friendship_id = friend["friendshipId"]
        .as_str()
        .or_else(|| friend["id"].as_str())
        .unwrap();
    api.delete_no_body(&format!("/friends/{}", friendship_id))
        .await?;
    println!("Removed {} from friends.", username);
    Ok(())
}

async fn show_requests(api: &mut ApiClient) -> Result<()> {
    let requests: serde_json::Value = api.get("/friends/requests").await?;

    let incoming = requests["incoming"].as_array();
    let outgoing = requests["outgoing"].as_array();

    if let Some(inc) = incoming {
        if !inc.is_empty() {
            println!("Incoming requests:");
            for req in inc {
                let username = req["from"]["username"]
                    .as_str()
                    .or_else(|| req["username"].as_str())
                    .unwrap_or("?");
                println!("  {} (use `lag friends accept {}`)", username, username);
            }
        }
    }

    if let Some(out) = outgoing {
        if !out.is_empty() {
            println!("Outgoing requests:");
            for req in out {
                let username = req["to"]["username"]
                    .as_str()
                    .or_else(|| req["username"].as_str())
                    .unwrap_or("?");
                println!("  {} (pending)", username);
            }
        }
    }

    let empty = incoming.is_none_or(|i| i.is_empty()) && outgoing.is_none_or(|o| o.is_empty());
    if empty {
        println!("No pending friend requests.");
    }

    Ok(())
}

async fn accept_request(api: &mut ApiClient, username: &str) -> Result<()> {
    let requests: serde_json::Value = api.get("/friends/requests").await?;
    let incoming = requests["incoming"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No incoming requests"))?;

    let req = incoming
        .iter()
        .find(|r| {
            r["from"]["username"].as_str() == Some(username)
                || r["username"].as_str() == Some(username)
        })
        .ok_or_else(|| anyhow::anyhow!("No request from '{}'", username))?;

    let request_id = req["id"].as_str().unwrap();
    let _: serde_json::Value = api
        .post(
            "/friends/accept",
            &serde_json::json!({ "requestId": request_id }),
        )
        .await?;
    println!("Accepted friend request from {}.", username);
    Ok(())
}

async fn decline_request(api: &mut ApiClient, username: &str) -> Result<()> {
    let requests: serde_json::Value = api.get("/friends/requests").await?;
    let incoming = requests["incoming"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No incoming requests"))?;

    let req = incoming
        .iter()
        .find(|r| {
            r["from"]["username"].as_str() == Some(username)
                || r["username"].as_str() == Some(username)
        })
        .ok_or_else(|| anyhow::anyhow!("No request from '{}'", username))?;

    let request_id = req["id"].as_str().unwrap();
    let _: serde_json::Value = api
        .post(
            "/friends/decline",
            &serde_json::json!({ "requestId": request_id }),
        )
        .await?;
    println!("Declined friend request from {}.", username);
    Ok(())
}
