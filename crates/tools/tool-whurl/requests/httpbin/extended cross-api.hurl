# @include:[quiet] dog facts/Dog breeds

# Builds on the basic request by reusing captured data.
GET https://httpbin.org/anything?source={{ first_breed_name }}
HTTP 200

[Asserts]
jsonpath "$.args.source" == "{{ first_breed_name }}"