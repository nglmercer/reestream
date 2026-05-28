use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub api_token: Option<String>,
    pub ip_allowlist: Vec<IpEntry>,
    pub ip_blocklist: Vec<IpEntry>,
    pub max_publishers: usize,
    pub per_platform_keys: bool,
    pub rate_limit_per_ip: u32,
    pub https_only: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            api_token: None,
            ip_allowlist: Vec::new(),
            ip_blocklist: Vec::new(),
            max_publishers: 10,
            per_platform_keys: false,
            rate_limit_per_ip: 10,
            https_only: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpEntry {
    pub ip: String,
    pub label: Option<String>,
}

impl IpEntry {
    pub fn matches(&self, addr: &IpAddr) -> bool {
        if self.ip.contains('/') {
            self.matches_cidr(addr)
        } else {
            self.ip == addr.to_string()
        }
    }

    fn matches_cidr(&self, addr: &IpAddr) -> bool {
        let parts: Vec<&str> = self.ip.split('/').collect();
        if parts.len() != 2 {
            return false;
        }
        let Ok(network): Result<IpAddr, _> = parts[0].parse() else {
            return false;
        };
        let Ok(prefix_len): Result<u8, _> = parts[1].parse() else {
            return false;
        };

        match (network, addr) {
            (IpAddr::V4(net), IpAddr::V4(addr)) => {
                let mask = !((1u32 << (32 - prefix_len)) - 1);
                let net_bits = u32::from_be_bytes(net.octets());
                let addr_bits = u32::from_be_bytes(addr.octets());
                (net_bits & mask) == (addr_bits & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(addr)) => {
                let net_bits = u128::from_be_bytes(net.octets());
                let addr_bits = u128::from_be_bytes(addr.octets());
                let mask = !((1u128 << (128 - prefix_len)) - 1);
                (net_bits & mask) == (addr_bits & mask)
            }
            _ => false,
        }
    }
}

pub struct IpFilter {
    config: SecurityConfig,
}

impl IpFilter {
    pub fn new(config: SecurityConfig) -> Self {
        Self { config }
    }

    pub fn is_allowed(&self, addr: &IpAddr) -> bool {
        if !self.config.ip_blocklist.is_empty() {
            for entry in &self.config.ip_blocklist {
                if entry.matches(addr) {
                    return false;
                }
            }
        }

        if !self.config.ip_allowlist.is_empty() {
            return self.config.ip_allowlist.iter().any(|e| e.matches(addr));
        }

        true
    }

    pub fn validate_api_token(&self, token: &str) -> bool {
        match &self.config.api_token {
            Some(expected) => token == expected,
            None => true,
        }
    }

    pub fn has_api_token(&self) -> bool {
        self.config.api_token.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeConfig {
    pub enabled: bool,
    pub domain: String,
    pub email: String,
    pub cert_dir: String,
    pub challenge_type: AcmeChallenge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AcmeChallenge {
    Http01,
    TlsAlpn01,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            domain: String::new(),
            email: String::new(),
            cert_dir: "/etc/reestream/certs".into(),
            challenge_type: AcmeChallenge::Http01,
        }
    }
}

impl AcmeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain.is_empty() {
            return Err("ACME domain cannot be empty".into());
        }
        if self.email.is_empty() {
            return Err("ACME email cannot be empty".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_config_default() {
        let config = SecurityConfig::default();
        assert!(config.api_token.is_none());
        assert!(config.ip_allowlist.is_empty());
        assert_eq!(config.max_publishers, 10);
    }

    #[test]
    fn test_ip_filter_allow_all() {
        let filter = IpFilter::new(SecurityConfig::default());
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(filter.is_allowed(&ip));
    }

    #[test]
    fn test_ip_filter_blocklist() {
        let config = SecurityConfig {
            ip_blocklist: vec![IpEntry {
                ip: "192.168.1.100".into(),
                label: Some("blocked".into()),
            }],
            ..Default::default()
        };
        let filter = IpFilter::new(config);
        let blocked: IpAddr = "192.168.1.100".parse().unwrap();
        let allowed: IpAddr = "192.168.1.101".parse().unwrap();
        assert!(!filter.is_allowed(&blocked));
        assert!(filter.is_allowed(&allowed));
    }

    #[test]
    fn test_ip_filter_allowlist() {
        let config = SecurityConfig {
            ip_allowlist: vec![IpEntry {
                ip: "10.0.0.0/8".into(),
                label: Some("internal".into()),
            }],
            ..Default::default()
        };
        let filter = IpFilter::new(config);
        let internal: IpAddr = "10.0.0.1".parse().unwrap();
        let external: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(filter.is_allowed(&internal));
        assert!(!filter.is_allowed(&external));
    }

    #[test]
    fn test_ip_entry_cidr_match() {
        let entry = IpEntry {
            ip: "192.168.1.0/24".into(),
            label: None,
        };
        let ip1: IpAddr = "192.168.1.50".parse().unwrap();
        let ip2: IpAddr = "192.168.2.1".parse().unwrap();
        assert!(entry.matches(&ip1));
        assert!(!entry.matches(&ip2));
    }

    #[test]
    fn test_api_token_validation() {
        let config = SecurityConfig {
            api_token: Some("secret123".into()),
            ..Default::default()
        };
        let filter = IpFilter::new(config);
        assert!(filter.validate_api_token("secret123"));
        assert!(!filter.validate_api_token("wrong"));
        assert!(filter.has_api_token());
    }

    #[test]
    fn test_api_token_none() {
        let filter = IpFilter::new(SecurityConfig::default());
        assert!(filter.validate_api_token("anything"));
        assert!(!filter.has_api_token());
    }

    #[test]
    fn test_acme_config_default() {
        let config = AcmeConfig::default();
        assert!(!config.enabled);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_acme_config_validate() {
        let config = AcmeConfig {
            enabled: true,
            domain: "example.com".into(),
            email: "admin@example.com".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }
}
