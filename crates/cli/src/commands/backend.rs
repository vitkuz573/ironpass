use crate::args::BackendAction;
use color_eyre::eyre;

pub async fn handle(action: BackendAction, json: bool) -> eyre::Result<()> {
    match action {
        BackendAction::List => list(json).await,
    }
}

async fn list(json: bool) -> eyre::Result<()> {
    let backends = vec![
        ("auto", "Automatically select the best backend for the node"),
        ("sing-box", "sing-box core (default for most nodes)"),
        ("xray", "Xray-core (default for XHTTP/Splithttp nodes)"),
    ];

    if json {
        let values: Vec<serde_json::Value> = backends
            .into_iter()
            .map(|(id, description)| {
                serde_json::json!({
                    "id": id,
                    "description": description,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&values)?);
    } else {
        println!("Supported proxy backends:");
        for (id, description) in backends {
            println!("  {:10} {}", id, description);
        }
    }

    Ok(())
}
