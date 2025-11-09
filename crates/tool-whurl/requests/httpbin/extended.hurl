# @include basic

# Builds on the basic request by reusing captured data.
GET https://httpbin.org/anything?source={{ response_url }}
HTTP 200

[Asserts]
jsonpath "$.args.source" == "{{ response_url }}"
jsonpath "$.url" == "https://httpbin.org/anything?source=https:%2F%2Fhttpbin.org%2Fget"