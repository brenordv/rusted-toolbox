//! DNS host checks for Azure service endpoints.

/// Returns `true` when `host` sits strictly under one of `suffixes` on a DNS
/// label boundary: `my-vault.vault.azure.net` is under `vault.azure.net`, while
/// `evilvault.azure.net`, `.vault.azure.net`, and the bare suffix itself are
/// not. Both sides are compared ASCII case-insensitively.
///
/// This is an allowlist check for hosts that receive Entra ID tokens: a token
/// minted for an Azure audience must only ever be presented to a host the
/// suffix list names.
pub fn host_has_allowed_suffix(host: &str, suffixes: &[&str]) -> bool {
    let host = host.to_ascii_lowercase();

    suffixes.iter().any(|suffix| {
        host.strip_suffix(suffix.to_ascii_lowercase().as_str())
            .and_then(|prefix| prefix.strip_suffix('.'))
            .is_some_and(|label| !label.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const SUFFIXES: [&str; 2] = ["vault.azure.net", "servicebus.windows.net"];

    #[rstest]
    #[case::direct_child("my-vault.vault.azure.net", true)]
    #[case::second_suffix("ns.servicebus.windows.net", true)]
    #[case::uppercase_host("MY-VAULT.VAULT.AZURE.NET", true)]
    #[case::deeper_child("a.b.vault.azure.net", true)]
    #[case::no_dot_boundary("evilvault.azure.net", false)]
    #[case::bare_suffix("vault.azure.net", false)]
    #[case::empty_label(".vault.azure.net", false)]
    #[case::unrelated_host("evil.example.com", false)]
    #[case::suffix_in_the_middle("vault.azure.net.evil.example.com", false)]
    #[case::trailing_dot("my-vault.vault.azure.net.", false)]
    #[case::empty_host("", false)]
    fn host_has_allowed_suffix_requires_a_label_boundary(
        #[case] host: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(host_has_allowed_suffix(host, &SUFFIXES), expected);
    }

    #[test]
    fn host_has_allowed_suffix_matches_an_uppercase_suffix() {
        assert!(host_has_allowed_suffix(
            "my-vault.vault.azure.net",
            &["VAULT.AZURE.NET"]
        ));
    }

    #[test]
    fn host_has_allowed_suffix_rejects_everything_for_an_empty_list() {
        assert!(!host_has_allowed_suffix("my-vault.vault.azure.net", &[]));
    }
}
