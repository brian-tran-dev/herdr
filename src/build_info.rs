//! Build identity helpers.

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn channel() -> &'static str {
    non_empty(option_env!("HERDR_BUILD_CHANNEL")).unwrap_or("stable")
}

pub fn build_id() -> Option<&'static str> {
    non_empty(option_env!("HERDR_BUILD_ID"))
}

pub fn version() -> String {
    match channel() {
        "stable" => BASE_VERSION.to_string(),
        channel => match build_id() {
            Some(build_id) => format!("{BASE_VERSION}-{channel}.{build_id}"),
            None => format!("{BASE_VERSION}-{channel}"),
        },
    }
}

/// Human-readable branding for this customized client, without changing the
/// base version or server/client compatibility metadata.
pub fn cli_version() -> String {
    format!("v{}-khoa-customized", version())
}

pub fn is_preview() -> bool {
    channel() == "preview"
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_version_defaults_to_cargo_version() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn customized_cli_version_preserves_runtime_version() {
        let runtime_version = super::version();
        assert_eq!(
            super::cli_version(),
            format!("v{runtime_version}-khoa-customized")
        );
        assert_eq!(super::BASE_VERSION, env!("CARGO_PKG_VERSION"));
        assert_eq!(super::version(), runtime_version);
    }
}
