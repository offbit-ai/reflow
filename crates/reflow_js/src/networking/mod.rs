pub mod fetch;
pub mod websocket;

pub use fetch::FetchModule;
pub use websocket::WebSocketModule;

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Maximum number of concurrent requests
    pub max_concurrent_requests: usize,
    
    /// Request timeout in seconds
    pub request_timeout: u64,
    
    /// Maximum response size in bytes
    pub max_response_size: usize,
    
    /// Whether to allow requests to localhost
    pub allow_localhost: bool,
    
    /// Whether to allow requests to private IP addresses
    pub allow_private_ip: bool,
    
    /// Allowed hosts (empty means all hosts are allowed)
    pub allowed_hosts: Vec<String>,
    
    /// Blocked hosts
    pub blocked_hosts: Vec<String>,
    
    /// Default headers to include in all requests
    pub default_headers: Vec<(String, String)>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            max_concurrent_requests: 10,
            request_timeout: 30,
            max_response_size: 10 * 1024 * 1024, // 10MB
            allow_localhost: true,
            allow_private_ip: false,
            allowed_hosts: vec![],
            blocked_hosts: vec![],
            default_headers: vec![
                ("User-Agent".to_string(), "Reflow JS Runtime".to_string()),
            ],
        }
    }
}

/// Check if a host is allowed
pub fn is_host_allowed(host: &str, config: &NetworkConfig) -> bool {
    // Check if the host is in the blocked hosts list
    if config.blocked_hosts.iter().any(|blocked| host.ends_with(blocked)) {
        return false;
    }
    
    // Check if the host is in the allowed hosts list
    if !config.allowed_hosts.is_empty() && !config.allowed_hosts.iter().any(|allowed| host.ends_with(allowed)) {
        return false;
    }
    
    // Check if the host is localhost
    if (host == "localhost" || host == "127.0.0.1" || host == "::1") && !config.allow_localhost {
        return false;
    }
    
    // Check if the host is a private IP address
    if !config.allow_private_ip {
        if let Ok(addr) = host.parse::<std::net::IpAddr>() {
            if !addr.is_global() {
                return false;
            }
        }
    }
    
    true
}
