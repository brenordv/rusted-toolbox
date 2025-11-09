# Demonstrates environment-specific payloads using `--env`.
GET https://httpbin.org/anything
[QueryStringParams]
environment: {{ env_name }}
message: {{ message }}
HTTP 200

[Asserts]
jsonpath "$.args.environment" == "{{ env_name }}"
jsonpath "$.args.message" == "{{ message }}"

