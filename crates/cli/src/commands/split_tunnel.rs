use crate::api_client::ApiClient;
use crate::output::print_json;
use color_eyre::eyre;
use uuid::Uuid;

pub async fn handle(
    url: &str,
    action: crate::args::SplitTunnelAction,
    json: bool,
) -> eyre::Result<()> {
    let client = ApiClient::with_url(url.into());

    match action {
        crate::args::SplitTunnelAction::List { node } => {
            let node_id = parse_optional_uuid(&node)?;
            let rules = client.list_split_tunnel_rules(node_id).await?;
            print_json(&rules, json)?;
        }
        crate::args::SplitTunnelAction::Add {
            target,
            value,
            action,
            node,
        } => {
            let node_id = parse_optional_uuid(&node)?;
            let rule = client
                .add_split_tunnel_rule(
                    map_target(target),
                    value,
                    map_action(action),
                    node_id,
                )
                .await?;
            print_json(&rule, json)?;
        }
        crate::args::SplitTunnelAction::Update {
            id,
            target,
            value,
            action,
            node,
        } => {
            let id = Uuid::parse_str(&id)?;
            let node_id = parse_optional_uuid(&node)?;
            let rule = client
                .update_split_tunnel_rule(
                    id,
                    map_target(target),
                    value,
                    map_action(action),
                    node_id,
                )
                .await?;
            print_json(&rule, json)?;
        }
        crate::args::SplitTunnelAction::Remove { id } => {
            let id = Uuid::parse_str(&id)?;
            let resp = client.delete_split_tunnel_rule(id).await?;
            print_json(&resp, json)?;
        }
    }

    Ok(())
}

fn parse_optional_uuid(value: &Option<String>) -> eyre::Result<Option<Uuid>> {
    match value {
        Some(s) => Ok(Some(Uuid::parse_str(s)?)),
        None => Ok(None),
    }
}

fn map_target(target: crate::args::SplitTunnelTargetArg) -> ironpass_api::models::SplitTunnelTarget {
    match target {
        crate::args::SplitTunnelTargetArg::Domain => ironpass_api::models::SplitTunnelTarget::Domain,
        crate::args::SplitTunnelTargetArg::Ip => ironpass_api::models::SplitTunnelTarget::Ip,
        crate::args::SplitTunnelTargetArg::Cidr => ironpass_api::models::SplitTunnelTarget::Cidr,
        crate::args::SplitTunnelTargetArg::App => ironpass_api::models::SplitTunnelTarget::App,
    }
}

fn map_action(action: crate::args::SplitTunnelActionArg) -> ironpass_api::models::SplitTunnelAction {
    match action {
        crate::args::SplitTunnelActionArg::Direct => ironpass_api::models::SplitTunnelAction::Direct,
        crate::args::SplitTunnelActionArg::Proxy => ironpass_api::models::SplitTunnelAction::Proxy,
    }
}
