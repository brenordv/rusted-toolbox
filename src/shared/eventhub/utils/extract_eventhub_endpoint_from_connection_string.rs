use crate::shared::utils::sanitize_string_for_filename::sanitize_string_for_filename;
use anyhow::{anyhow, Context, Result};
use url::Url;

/// Extracts the Event Hub endpoint's hostname from a given connection string.
///
/// # Arguments
///
/// * `connection_string` - A reference to a string slice containing the Event Hub connection string.
///   The connection string is expected to have key-value pairs separated by semicolons (e.g., `Endpoint=sb://<namespace>.servicebus.windows.net/;SharedAccessKeyName=...`).
///
/// # Returns
///
/// Returns a `Result<String>`:
///
/// - `Ok(String)` containing the sanitized hostname of the Event Hub endpoint if successfully extracted.
/// - `Err(anyhow::Error)` if the connection string does not contain a valid `Endpoint=...` key or if the endpoint URL is invalid.
///
/// The returned hostname is sanitized to ensure that it is safe for use as a cross-platform filename.
///
/// # Errors
///
/// This function will return an error in the following scenarios:
///
/// - If the connection string does not contain an `Endpoint=` key, or it is improperly formatted.
/// - If the value for `Endpoint=` is an invalid URL that cannot be parsed.
/// - If the URL does not contain a valid hostname.
///
/// In the above example, `myeventhub.servicebus.windows.net` would be extracted and returned as the endpoint hostname.
///
/// # Notes
///
/// This function relies on `Url::parse` from the `url` crate for parsing the URL, and uses a custom `sanitize_string_for_filename`
/// utility for making the extracted hostname safe for use as a filename. Ensure these dependencies are included in your project.
pub fn extract_eventhub_endpoint_from_connection_string(connection_string: &str) -> Result<String> {
    // Parse connection string to extract endpoint hostname
    let mut endpoint = String::new();

    for part in connection_string.trim().split(';') {
        if part.starts_with("Endpoint=") {
            if let Some(endpoint_url) = part.strip_prefix("Endpoint=") {
                // Parse URL to extract hostname
                let url = Url::parse(endpoint_url).context(format!(
                    "Invalid endpoint URL in connection string: {}",
                    endpoint_url
                ))?;

                endpoint = url
                    .host_str()
                    .context("No hostname found in endpoint URL")?
                    .to_string();
                break;
            }
        }
    }

    if endpoint.is_empty() {
        return Err(anyhow!("No valid endpoint found in connection string"));
    }

    // Sanitize for cross-platform filename safety
    Ok(sanitize_string_for_filename(&endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("Endpoint=sb://example.servicebus.windows.net/;SharedAccessKeyName=key;SharedAccessKey=secret;", "example.servicebus.windows.net")]
    #[case(
        "Endpoint=sb://example.servicebus.windows.net:443/;SharedAccessKeyName=key;",
        "example.servicebus.windows.net"
    )]
    #[case(
        "SharedAccessKeyName=key;Endpoint=sb://test.eventhub.azure.com/;SharedAccessKey=secret;",
        "test.eventhub.azure.com"
    )]
    #[case("Endpoint=sb://first.servicebus.windows.net/;Endpoint=sb://second.servicebus.windows.net/;SharedAccessKeyName=key;", "first.servicebus.windows.net")]
    #[case(
        " Endpoint=sb://example.servicebus.windows.net/; SharedAccessKeyName=key; ",
        "example.servicebus.windows.net"
    )]
    #[case(
        "Endpoint=sb://example.servicebus.windows.net/",
        "example.servicebus.windows.net"
    )]
    #[case(
        "Endpoint=sb://example.servicebus.windows.net/;",
        "example.servicebus.windows.net"
    )]
    #[case(
        ";;Endpoint=sb://example.servicebus.windows.net/;;SharedAccessKeyName=key;;",
        "example.servicebus.windows.net"
    )]
    #[case(
        "Endpoint=sb://192.168.1.100:5671/;SharedAccessKeyName=key;",
        "192.168.1.100"
    )]
    #[case("Endpoint=sb://localhost:5671/;SharedAccessKeyName=key;", "localhost")]
    #[case("Endpoint=sb://myeventhub.servicebus.windows.net/;SharedAccessKeyName=RootManageSharedAccessKey;SharedAccessKey=someKey123=;EntityPath=myeventhub", "myeventhub.servicebus.windows.net")]
    #[case(
        "Endpoint=sb://example.servicebus.windows.net/some/path;SharedAccessKeyName=key;",
        "example.servicebus.windows.net"
    )]
    #[case(
        "Endpoint=sb://example.servicebus.windows.net/?query=param;SharedAccessKeyName=key;",
        "example.servicebus.windows.net"
    )]
    #[case(
        "Endpoint=sb://example.servicebus.windows.net/#fragment;SharedAccessKeyName=key;",
        "example.servicebus.windows.net"
    )]
    fn test_extract_eventhub_endpoint_from_connection_string_success(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let result = extract_eventhub_endpoint_from_connection_string(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[rstest]
    #[case("Endpoint=sb://test:host<name>.service/bus.windows.net/;SharedAccessKeyName=key;")]
    #[case("Endpoint=;SharedAccessKeyName=key;")]
    #[case("Endpoint=not-a-valid-url;SharedAccessKeyName=key;")]
    fn test_extract_eventhub_endpoint_from_connection_string_failure_invalid_url(
        #[case] input: &str,
    ) {
        let result = extract_eventhub_endpoint_from_connection_string(input);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid endpoint URL"));
    }

    #[test]
    fn test_extract_eventhub_endpoint_from_connection_string_failure_not_host_name() {
        let result = extract_eventhub_endpoint_from_connection_string(
            "Endpoint=file:///local/path;SharedAccessKeyName=key;",
        );
        dbg!(&result);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No hostname found in endpoint URL"));
    }

    #[rstest]
    #[case("")]
    #[case("SharedAccessKeyName=key;SharedAccessKey=secret;EntityPath=test;")]
    #[case("Endpoin=sb://example.servicebus.windows.net/;SharedAccessKeyName=key;")]
    #[case("endpoint=sb://example.servicebus.windows.net/;SharedAccessKeyName=key;")]
    fn test_extract_eventhub_endpoint_from_connection_string_failure_no_endpoint(
        #[case] input: &str,
    ) {
        let result = extract_eventhub_endpoint_from_connection_string(input);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No valid endpoint found"));
    }
}
