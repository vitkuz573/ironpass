use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Parser)]
#[command(
    name = "ironpass",
    about = "Enterprise VPN client",
    version,
    long_about = "IronPass — thin CLI for the ironpassd REST API."
)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true, help = "ironpassd API URL")]
    pub api_url: Option<String>,

    #[arg(long, global = true, help = "Start daemon automatically if not running")]
    pub auto_start: bool,

    #[arg(long, global = true)]
    pub config: Option<String>,

    #[arg(long, global = true, short = 'v')]
    pub verbose: bool,

    #[arg(long, global = true)]
    pub quiet: bool,

    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Control the ironpassd daemon")]
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    #[command(about = "Fetch and display subscription nodes")]
    Fetch {
        #[arg(help = "Subscription URL")]
        url: Option<String>,

        #[arg(long, short = 'o', help = "Output format")]
        format: Option<OutputFormatArg>,

        #[arg(long, short = 'O', help = "Write output to file")]
        output: Option<String>,

        #[arg(long, help = "Override HWID")]
        hwid: Option<String>,

        #[arg(long, help = "Include placeholder/dummy nodes")]
        include_placeholders: bool,

        #[arg(long, help = "Sort nodes by field")]
        sort: Option<String>,
    },

    #[command(about = "Manage subscriptions")]
    Sub {
        #[command(subcommand)]
        action: SubAction,
    },

    #[command(about = "Generate and manage HWID")]
    Hwid {
        #[command(subcommand)]
        action: HwidAction,
    },

    #[command(about = "Convert between subscription formats")]
    Convert {
        #[arg(help = "Input file (stdin if omitted)")]
        input: Option<String>,

        #[arg(long, short = 'f', help = "Input format (auto-detected if omitted)")]
        from: Option<FormatHint>,

        #[arg(long, short = 't', help = "Output format")]
        to: OutputFormatArg,

        #[arg(long, short = 'O', help = "Output file")]
        output: Option<String>,
    },

    #[command(about = "Analyze subscription (health, protocols, geolocation)")]
    Analyze {
        #[arg(help = "Subscription URL or ID")]
        target: Option<String>,

        #[arg(long, help = "Run connectivity probes")]
        probe: bool,

        #[arg(long, help = "Show detailed node info")]
        detailed: bool,
    },

    #[command(about = "Export subscription for specific client")]
    Export {
        #[arg(help = "Subscription URL or ID")]
        target: Option<String>,

        #[arg(long, short = 't', help = "Target client")]
        target_client: ExportTarget,

        #[arg(long, short = 'O', help = "Output file")]
        output: Option<String>,

        #[arg(long, help = "Override HWID")]
        hwid: Option<String>,
    },

    #[command(about = "Generate shell completions")]
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },

    #[command(about = "Show current configuration")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    #[command(about = "Check connectivity to subscription server")]
    Ping {
        #[arg(help = "Subscription URL")]
        url: String,

        #[arg(long, help = "Timeout in seconds")]
        timeout: Option<u64>,
    },

    #[command(about = "Start proxy through VPN tunnel")]
    Proxy {
        #[arg(help = "Node ID (or selected node if omitted)")]
        node: Option<String>,

        #[arg(long, default_value = "11080", help = "Local mixed/SOCKS port")]
        socks_port: u16,

        #[arg(long, default_value = "11080", help = "Local HTTP port")]
        http_port: u16,

        #[arg(long, help = "Use mixed inbound on this port")]
        mixed_port: Option<u16>,
    },

    #[command(about = "Manage split tunnel (selective routing) rules")]
    SplitTunnel {
        #[command(subcommand)]
        action: SplitTunnelAction,
    },
}

#[derive(Subcommand)]
pub enum SplitTunnelAction {
    #[command(about = "List split tunnel rules")]
    List {
        #[arg(long, help = "Filter rules by node ID")]
        node: Option<String>,
    },

    #[command(about = "Add a split tunnel rule")]
    Add {
        #[arg(value_enum)]
        target: SplitTunnelTargetArg,

        #[arg(help = "Rule value (domain, IP, CIDR or app path)")]
        value: String,

        #[arg(value_enum)]
        action: SplitTunnelActionArg,

        #[arg(long, help = "Associate rule with a specific node")]
        node: Option<String>,
    },

    #[command(about = "Update a split tunnel rule")]
    Update {
        #[arg(help = "Rule ID")]
        id: String,

        #[arg(value_enum)]
        target: SplitTunnelTargetArg,

        #[arg(help = "Rule value")]
        value: String,

        #[arg(value_enum)]
        action: SplitTunnelActionArg,

        #[arg(long, help = "Associated node ID")]
        node: Option<String>,
    },

    #[command(about = "Remove a split tunnel rule")]
    Remove {
        #[arg(help = "Rule ID")]
        id: String,
    },
}

#[derive(Clone, ValueEnum)]
pub enum SplitTunnelTargetArg {
    Domain,
    Ip,
    Cidr,
    App,
}

#[derive(Clone, ValueEnum)]
pub enum SplitTunnelActionArg {
    Direct,
    Proxy,
}

#[derive(Subcommand)]
pub enum DaemonAction {
    Start,
    Stop,
    Status,
}

#[derive(Subcommand)]
pub enum SubAction {
    #[command(about = "Add a subscription")]
    Add {
        #[arg(help = "Subscription URL")]
        url: String,

        #[arg(long, short = 'n', help = "Display name")]
        name: Option<String>,

        #[arg(long, help = "HWID to use for this subscription")]
        hwid: Option<String>,
    },

    #[command(about = "Remove a subscription")]
    Remove {
        #[arg(help = "Subscription URL, name or ID")]
        target: String,
    },

    #[command(about = "List all saved subscriptions")]
    List {
        #[arg(long, help = "Show detailed info")]
        detailed: bool,
    },

    #[command(about = "Update (re-fetch) a subscription")]
    Update {
        #[arg(help = "Subscription URL, name or ID (all if omitted)")]
        target: Option<String>,

        #[arg(long, help = "Override HWID")]
        hwid: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum HwidAction {
    #[command(about = "Show current HWID")]
    Show,

    #[command(about = "Regenerate HWID")]
    Regenerate,

    #[command(about = "Show detailed device info")]
    Info,

    #[command(about = "Set custom HWID")]
    Set {
        #[arg(help = "Custom HWID value")]
        value: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    #[command(about = "Show current config")]
    Show,

    #[command(about = "Reset config to defaults")]
    Reset,

    #[command(about = "Set a config value")]
    Set {
        #[arg(help = "Config key (e.g. general.user_agent)")]
        key: String,

        #[arg(help = "Value")]
        value: String,
    },

    #[command(about = "Show config file paths")]
    Paths,
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormatArg {
    #[value(alias = "clash")]
    Clash,
    #[value(alias = "sb", alias = "singbox")]
    SingBox,
    #[value(alias = "v2ray")]
    V2Ray,
    #[value(alias = "raw")]
    Raw,
    #[value(alias = "json")]
    Json,
    #[value(alias = "table")]
    Table,
}

#[derive(Clone, ValueEnum)]
pub enum FormatHint {
    #[value(alias = "base64")]
    Base64,
    #[value(alias = "clash")]
    Clash,
    #[value(alias = "singbox", alias = "sb")]
    SingBox,
    #[value(alias = "raw", alias = "uri")]
    Raw,
    #[value(alias = "auto")]
    Auto,
}

#[derive(Clone, ValueEnum)]
pub enum ExportTarget {
    #[value(alias = "clash")]
    Clash,
    #[value(alias = "clash-meta", alias = "mihomo")]
    ClashMeta,
    #[value(alias = "singbox", alias = "sb")]
    SingBox,
    #[value(alias = "v2rayn")]
    V2RayN,
    #[value(alias = "v2rayng")]
    V2RayNG,
    #[value(alias = "hiddify")]
    Hiddify,
    #[value(alias = "nekoray")]
    NekoRay,
    #[value(alias = "surge")]
    Surge,
    #[value(alias = "shadowrocket")]
    Shadowrocket,
    #[value(alias = "quantumult")]
    QuantumultX,
    #[value(alias = "loone")]
    Loon,
}
