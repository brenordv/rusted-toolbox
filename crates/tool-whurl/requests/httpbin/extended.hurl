# @include basic
# @vars session


# Builds on the basic request by reusing captured data.
GET https://httpbin.org/anything
[QueryStringParams]
source: {{ response_url }}
call_id: {{ call_id }}
first_name: {{ first_name }}
food: {{ food }}
score: {{ score }}

HTTP 200

[Asserts]
jsonpath "$.args.source" == "{{ response_url }}"
jsonpath "$.args.call_id" matches ".+"
jsonpath "$.url" matches "https://httpbin.org/anything\\?.*source=.*"
jsonpath "$.args.first_name" == "{{ first_name }}"