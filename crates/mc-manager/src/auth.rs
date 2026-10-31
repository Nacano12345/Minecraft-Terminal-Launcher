use mc_core::Error;

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
}

impl Profile {
    /// Offline UUID matching vanilla's `OfflinePlayer:<name>` MD5 scheme, so
    /// offline servers recognise the player.
    pub fn offline(name: &str) -> Self {
        let digest = md5::compute(format!("OfflinePlayer:{name}"));
        let hex = hex::encode(digest.0);
        let uuid = format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        );
        Self {
            name: name.to_string(),
            uuid,
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
        }
    }
}

/// Placeholder for the Microsoft device-code flow (M4).
pub fn microsoft_unsupported() -> Error {
    Error::Other("Microsoft login is not implemented yet; use --username for offline mode".into())
}
