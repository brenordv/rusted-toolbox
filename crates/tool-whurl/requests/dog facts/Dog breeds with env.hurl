# Get request that fetches a list of dog breeds
# @include httpbin/env-demo

GET https://dogapi.dog/api/v2/breeds
[Query]
foo={{dog_facts}}

HTTP 200

[Asserts]
status == 200
jsonpath "$.data" count > 0

[Captures]
status_code: status
first_breed: jsonpath "$.data[0]"
first_breed_name: jsonpath "$.data[0].attributes.name"
all_breeds: jsonpath "$.data"