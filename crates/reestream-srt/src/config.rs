use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtConfig {
    pub enabled: bool,
    pub listen_addr: String,
    pub listen_port: u16,
    pub latency_ms: u32,
    pub max_bandwidth: i64,
    pub passphrase: Option<String>,
    pub pbkey_len: Option<u32>,
}

impl Default for SrtConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: "0.0.0.0".into(),
            listen_port: 3000,
            latency_ms: 200,
            max_bandwidth: -1,
            passphrase: None,
            pbkey_len: None,
        }
    }
}

impl SrtConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_port == 0 {
            return Err("SRT listen_port cannot be 0".into());
        }
        if self.listen_addr.is_empty() {
            return Err("SRT listen_addr cannot be empty".into());
        }
        if self.latency_ms == 0 {
            return Err("SRT latency_ms cannot be 0".into());
        }
        if self.enabled && self.passphrase.is_none() {
            return Err("SRT passphrase is required when SRT is enabled".into());
        }
        if let Some(ref pass) = self.passphrase
            && pass.len() < 10
        {
            return Err("SRT passphrase must be at least 10 characters".into());
        }
        if let Some(len) = self.pbkey_len
            && len != 16
            && len != 24
            && len != 32
        {
            return Err("SRT pbkey_len must be 16, 24, or 32".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SrtConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.listen_addr, "0.0.0.0");
        assert_eq!(config.listen_port, 3000);
        assert_eq!(config.latency_ms, 200);
        assert_eq!(config.max_bandwidth, -1);
        assert!(config.passphrase.is_none());
    }

    #[test]
    fn test_validate_ok() {
        let config = SrtConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_port() {
        let config = SrtConfig {
            listen_port: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_empty_addr() {
        let config = SrtConfig {
            listen_addr: "".into(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_latency() {
        let config = SrtConfig {
            latency_ms: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_short_passphrase() {
        let config = SrtConfig {
            passphrase: Some("short".into()),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_enabled_requires_passphrase() {
        let config = SrtConfig {
            enabled: true,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_passphrase() {
        let config = SrtConfig {
            passphrase: Some("longenoughpassphrase".into()),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_pbkey_len() {
        let config = SrtConfig {
            pbkey_len: Some(12),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_pbkey_len() {
        for len in [16, 24, 32] {
            let config = SrtConfig {
                pbkey_len: Some(len),
                ..Default::default()
            };
            assert!(config.validate().is_ok(), "pbkey_len={len} should be valid");
        }
    }

    #[test]
    fn test_config_clone() {
        let config = SrtConfig::default();
        let cloned = config.clone();
        assert_eq!(config.listen_port, cloned.listen_port);
    }

    #[test]
    fn test_config_serialize() {
        let config = SrtConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("3000"));
        assert!(json.contains("200"));
    }
}
