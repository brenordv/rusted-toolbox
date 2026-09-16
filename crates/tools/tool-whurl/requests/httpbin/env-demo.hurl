# Demonstrates environment-specific payloads using `--env`.
GET https://httpbin.org/anything
[QueryStringParams]
environment: {{ env_name }}
message: {{ message }}
get_answer: {{ question }}
timestamp: {{ timestamp }}
request_id: {{ request_id }}

HTTP 200

[Asserts]
jsonpath "$.args.environment" == "{{ env_name }}"
jsonpath "$.args.message" == "{{ message }}"
jsonpath "$.args.timestamp" matches ".+"