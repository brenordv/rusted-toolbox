# Basic GET request to httpbin. Asserts success and captures returned data.
GET https://httpbin.org/get
HTTP 200

[Asserts]
jsonpath "$.url" == "https://httpbin.org/get"
status >= 200
status < 300

[Captures]
status_code: status
response_url: jsonpath "$.url"