# Get request that fetches a list of dog breeds
GET https://dogapi.dog/api/v2/breeds
HTTP 200

[Asserts]
status == 200
jsonpath "$.data" count > 0

[Captures]
status_code: status
first_breed: jsonpath "$.data[0]"
first_breed_name: jsonpath "$.data[0].attributes.name"
all_breeds: jsonpath "$.data"