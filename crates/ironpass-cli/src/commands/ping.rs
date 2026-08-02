use color_eyre::eyre;
use std::time::{Duration, Instant};

pub async fn handle(url: String, timeout: Option<u64>) -> eyre::Result<()> {
    let timeout = Duration::from_secs(timeout.unwrap_or(10));

    println!("Pinging: {}", url);

    let client = reqwest::Client::builder().timeout(timeout).build()?;

    let start = Instant::now();
    let response = client.get(&url).send().await?;
    let elapsed = start.elapsed();

    println!("Status:   {}", response.status());
    println!("Latency:  {}ms", elapsed.as_millis());

    let headers = response.headers();
    if let Some(ct) = headers.get("content-type") {
        println!("Type:     {}", ct.to_str().unwrap_or("unknown"));
    }
    if let Some(server) = headers.get("server") {
        println!("Server:   {}", server.to_str().unwrap_or("unknown"));
    }

    let body = response.text().await?;
    println!("Size:     {} bytes", body.len());

    if body.len() < 500 {
        println!();
        println!("Response:");
        println!("{}", body);
    } else {
        println!();
        println!("First 200 bytes:");
        println!("{}", &body[..200]);
    }

    Ok(())
}
