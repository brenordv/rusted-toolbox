# mock-data-utils

Mock data generators behind `tool-mock`, and message templates filled with them.
`generate_mock_data(&MockOptions)` validates the options and dispatches to one of 32
generators in four groups (the dotted command names are `DataType::command()`, listed in
`DataType::ALL`):

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
| `length`        | `internet.password`, `commerce.product-description`; zero yields an empty string, above 1 MiB (`MAX_LENGTH`) is refused |
| `age`           | `person.birthday` (target age in years)                                                            |
| `past`/`future` | `random.date`, `random.time`, `random.datetime`, `random.timestamp`; `past` wins when both are set |
| `range`         | `random.date`, `random.datetime`, `random.timestamp` (half-width in years, minimum 1)              |
| `locale`        | the fake-backed types: `person.*` except birthday, `internet.username`, `commerce.company`, `commerce.job-title`, `commerce.industry` |

## Locales

`Locale` covers the 14 locales fake 5.1.0 ships: `en` (default), `ar-sa`, `cy-gb`, `de-de`,
`fa-ir`, `fr-fr`, `it-it`, `ja-jp`, `nl-nl`, `pt-br`, `pt-pt`, `tr-tr`, `zh-cn`, `zh-tw`.
`Locale::from_code` parses case-insensitively with `-` and `_` interchangeable.
`DataType::is_locale_aware` reports whether a type honors the locale. fake falls back to its
English data for any category a locale does not localize, so every locale is valid for every
type. `person.email` localizes the name parts while its domain list stays fixed, so non-Latin
locales produce non-ASCII local parts (mock data, matching fake's own localized name data).

## Templates

`template::Template::compile(source)` turns text with placeholders into a template, and
`render(max_bytes)` produces one instance with fresh values:

```
{"orderId": {{random.integer min=1000 max=9999}}, "email": "{{person.email}}", "at": "{{random.datetime past}}"}
```

- A placeholder is `{{type option=value ...}}`: a command name, then options as words (`min=N`,
  `max=N`, `length=N`, `precision=N`, `age=N`, `range=N`, `locale=CODE`, and the bare `past`
  and `future`). Whitespace and newlines inside the braces don't matter. `OPTIONS_HELP` and
  `placeholder_groups()` hold the help text for the syntax.
- `\{{` is a literal `{{`; every other character, `}}` and other backslashes included, is copied
  as written.
- Compiling checks each placeholder the way `generate_mock_data` would (unknown type or option,
  a value that does not parse, an option twice, a value on `past`/`future`, the validation
  rules above) and refuses more than 100,000 placeholders. Errors start with the placeholder's
  line and column (the column counts characters) and quote at most 40 characters of the text.
- A non-default `locale` on a type that ignores it is not an error; `inert_locale_notes()`
  returns one note per type and locale pair (`locale_is_inert` is the rule).
- `render` fails once the output passes `max_bytes`, naming the cap and, when a placeholder's
  value crossed it, that placeholder by position and type; it never quotes the template.
  Values are inserted as generated, with no escaping: fake's pt-br last names include
  `D"ávila`, which breaks a JSON string.
- `Debug` of a template shows its counts only.

## Contracts

- `generate_mock_data` rejects invalid options (`min > max`, `range` of zero)
  with an error instead of panicking. The individual generator functions assume
  pre-validated options; calling one directly with out-of-range values can
  panic in the random-range sampling.
- `internet.password` produces mock/test data from an alphanumeric-plus-symbols
  charset. It is not a credential generator; do not use it for real secrets.