# mock-data-utils

Mock data generators behind `tool-mock`. `generate_mock_data(&MockOptions)`
validates the options and dispatches to one of 32 generators in four groups
(the dotted command names come from `DataType::from_command`):

- `person.*`: first/last/full name, email, phone, street, city, state,
  country, postal code, full address, birthday.
- `internet.*`: username, password, URL, image URL, file URL.
- `random.*`: date, time, datetime, timestamp, hex/RGB color, integer, float,
  car brand.
- `commerce.*`: company, product, product description, job title, industry,
  buzzword.

## Options

Each option is honored only by the generators listed; the rest ignore it.

| Option          | Honored by                                                                                         |
|-----------------|----------------------------------------------------------------------------------------------------|
| `min`/`max`     | `random.integer`, `random.float` (inclusive bounds)                                                |
| `precision`     | `random.float` (decimal places)                                                                    |
| `length`        | `internet.password`, `commerce.product-description`; zero yields an empty string                   |
| `age`           | `person.birthday` (target age in years)                                                            |
| `past`/`future` | `random.date`, `random.time`, `random.datetime`, `random.timestamp`; `past` wins when both are set |
| `range`         | `random.date`, `random.datetime`, `random.timestamp` (half-width in years, minimum 1)              |

## Contracts

- `generate_mock_data` rejects invalid options (`min > max`, `range` of zero)
  with an error instead of panicking. The individual generator functions assume
  pre-validated options; calling one directly with out-of-range values can
  panic in the random-range sampling.
- `internet.password` produces mock/test data from an alphanumeric-plus-symbols
  charset. It is not a credential generator; do not use it for real secrets.