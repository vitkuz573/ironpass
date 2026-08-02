use ironpass_core::models::{ProxyNode, Subscription};
use ironpass_config::StoredSubscription;
use console::style;
use std::io::{self, Write};

fn display_width(s: &str) -> usize {
    s.chars().map(|c| {
        if unicode_is_wide(c) { 2 } else { 1 }
    }).sum()
}

fn unicode_is_wide(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x1100..=0x115F |
        0x2E80..=0x303E |
        0x3041..=0x33BF |
        0x3400..=0x4DBF |
        0x4E00..=0x9FFF |
        0xA000..=0xA4CF |
        0xAC00..=0xD7AF |
        0xF900..=0xFAFF |
        0xFE30..=0xFE6F |
        0xFF01..=0xFF60 |
        0xFFE0..=0xFFE6 |
        0x20000..=0x2FA1F |
        0x30000..=0x3134F |
        // Emoji flags and symbols
        0x1F1E0..=0x1F1FF |  // Regional indicator symbols (flags)
        0x1F300..=0x1F9FF |  // Misc Symbols and Pictographs
        0x1FA00..=0x1FA6F |  // Chess Symbols
        0x1FA70..=0x1FAFF |  // Symbols and Pictographs Extended-A
        0x2600..=0x26FF |    // Misc Symbols
        0x2700..=0x27BF |    // Dingbats
        0x2300..=0x23FF      // Misc Technical
    )
}

fn pad_to_width(s: &str, target_width: usize) -> String {
    let current = display_width(s);
    if current >= target_width {
        truncate_to_width(s, target_width)
    } else {
        let padding = target_width - current;
        format!("{}{}", s, " ".repeat(padding))
    }
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;
    for c in s.chars() {
        let w = if unicode_is_wide(c) { 2 } else { 1 };
        if current_width + w > max_width.saturating_sub(3) {
            result.push_str("...");
            break;
        }
        result.push(c);
        current_width += w;
    }
    result
}

pub fn print_nodes_table(nodes: &[ProxyNode], sub: &Subscription) -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    writeln!(handle, "{}", style("IronPass Subscription").bold())?;
    writeln!(handle, "{}", style("─".repeat(90)).dim())?;
    writeln!(handle, "  URL:        {}", sub.url)?;
    writeln!(handle, "  Fetched:    {}", sub.fetched_at.format("%Y-%m-%d %H:%M:%S UTC"))?;
    if let Some(used) = sub.traffic_used {
        writeln!(handle, "  Traffic:    {} used", bytesize::to_string(used, true))?;
    }
    if let Some(total) = sub.traffic_total {
        writeln!(handle, "    Total:    {}", bytesize::to_string(total, true))?;
    }
    writeln!(handle, "  Nodes:      {}", nodes.len())?;
    writeln!(handle, "{}", style("─".repeat(90)).dim())?;
    writeln!(handle)?;

    let header = format!(
        "{:<4} {:<30} {:<22} {:<10} {:<8} {:<8}",
        "#", "NAME", "SERVER", "PROTO", "TRANS", "SEC"
    );
    writeln!(handle, "{}", style(header).bold())?;
    writeln!(handle, "{}", style("─".repeat(90)).dim())?;

    for (i, node) in nodes.iter().enumerate() {
        let name_truncated = truncate_to_width(&node.name, 30);
        let name_padded = pad_to_width(&name_truncated, 30);

        let server_str = format!("{}:{}", node.server, node.port);
        let server_padded = pad_to_width(&server_str, 22);

        let proto = format!("{:?}", node.protocol);
        let trans = format!("{:?}", node.transport);
        let sec = format!("{:?}", node.security);

        let row = format!(
            "{:<4} {} {} {:<10} {:<8} {:<8}",
            i + 1, name_padded, server_padded, proto, trans, sec
        );
        writeln!(handle, "{}", row)?;
    }

    Ok(())
}

pub fn print_nodes_json(nodes: &[ProxyNode]) -> io::Result<()> {
    println!("{}", serde_json::to_string_pretty(nodes).unwrap());
    Ok(())
}

pub fn print_subscriptions(subs: &[StoredSubscription], detailed: bool) {
    println!("{}", style("Saved Subscriptions").bold());
    println!("{}", style("─".repeat(70)).dim());

    for (i, sub) in subs.iter().enumerate() {
        let status = if sub.is_active { style("ACTIVE").green() } else { style("INACTIVE").red() };

        println!("  {}. {} [{}]", i + 1, sub.url, status);

        if let Some(ref name) = sub.name {
            println!("     Name:     {}", name);
        }
        if detailed {
            println!("     Added:    {}", sub.added_at.format("%Y-%m-%d %H:%M UTC"));
            if let Some(ref last) = sub.last_updated {
                println!("     Updated:  {}", last.format("%Y-%m-%d %H:%M UTC"));
            }
            if let Some(ref hwid) = sub.hwid {
                println!("     HWID:     {}...", &hwid[..hwid.len().min(16)]);
            }
        }
    }
}
