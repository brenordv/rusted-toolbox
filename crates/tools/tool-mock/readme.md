# Mock Data Generator CLI Tool

A fast and intuitive CLI tool for generating various types of mock data for testing and development purposes.

## Features

- **Personal Information**: Names, emails, addresses, phone numbers, birthdays
- **Internet & Tech**: Usernames, passwords, URLs, image URLs, file URLs
- **Random Data**: Dates, times, colors, integers, floats, car brands
- **Commerce**: Company names, products, job titles, industries, buzzwords
- **Flexible Options**: Support for ranges, locales, and data constraints
- **Simple Output**: Only prints the requested value, no extra formatting
- **Fast Execution**: Quick startup and generation

## Usage
### Basic Syntax
```bash
mock [DATA_TYPE] [OPTIONS]
```

### Examples
```bash
# Personal information
mock person.first-name
mock person.email
mock person.address

# Internet & tech
mock internet.username
mock internet.password --length 16
mock internet.url

# Random data
mock random.integer --min 1 --max 100
mock random.date --past
mock random.color-hex
mock random.car-brand

# Commerce
mock commerce.company
mock commerce.product-description --length 200
mock commerce.job-title
```

## Available Data Types

### Personal Information
- `person.first-name` - Generate a random first name
- `person.last-name` - Generate a random last name
- `person.full-name` - Generate a full name (first + last)
- `person.email` - Generate a random email address
- `person.phone` - Generate a phone number
- `person.street` - Generate a street name
- `person.city` - Generate a city name
- `person.state` - Generate a state name
- `person.country` - Generate a country name
- `person.postal-code` - Generate postal/zip code
- `person.address` - Generate a full address
- `person.birthday` - Generate a birthday (with optional `--age` parameter)

### Internet & Tech
- `internet.username` - Generate a username
- `internet.password` - Generate a password (with `--length` option)
- `internet.url` - Generate a URL
- `internet.image-url` - Generate an image URL
- `internet.file-url` - Generate a file URL

### Random Data
- `random.date` - Generate a date (with `--past`, `--future`, or `--range` options)
- `random.time` - Generate a time (with `--past`, `--future` options)
- `random.datetime` - Generate a datetime (with `--past`, `--future` options)
- `random.timestamp` - Generate a timestamp (with `--past`, `--future` options)
- `random.color-hex` - Generate a hex color code
- `random.color-rgb` - Generate RGB color values
- `random.integer` - Generate an integer (with `--min`, `--max` options)
- `random.float` - Generate a float (with `--min`, `--max`, `--precision` options)
- `random.car-brand` - Generate a car brand name

### Commerce
- `commerce.company` - Generate a company name
- `commerce.product` - Generate a product name
- `commerce.product-description` - Generate a product description
- `commerce.job-title` - Generate a job title
- `commerce.industry` - Generate an industry name
- `commerce.buzzword` - Generate a business buzzword

## Options
### Global Options
- `--help, -h` - Show help
- `--version, -V` - Show version
- `--locale <LOCALE>` - Set locale for region-specific data (default: en_US)

### Data-specific Options
- `--min <NUMBER>` - Minimum value (for numbers)
- `--max <NUMBER>` - Maximum value (for numbers)
- `--length <NUMBER>` - Length specification (for passwords, etc.)
- `--precision <NUMBER>` - Decimal precision (for floats)
- `--age <NUMBER>` - Age for birthday calculation
- `--past` - Generate past dates/times
- `--future` - Generate future dates/times
- `--range <YEARS>` - Date range in years (default: 50)

## Examples with Options
```bash
# Generate a password with specific length
mock internet.password --length 20

# Generate an integer within a range
mock random.integer --min 10 --max 100

# Generate a float with precision
mock random.float --min 0 --max 1 --precision 4

# Generate a past date within 10 years
mock random.date --past --range 10

# Generate a birthday for a 25-year-old
mock person.birthday --age 25

# Generate a product description with specific length
mock commerce.product-description --length 150
```

## Use Cases

- **Testing**: Generate test data for applications
- **Development**: Populate databases with realistic mock data
- **Prototyping**: Create sample content for UI mockups
- **Scripting**: Generate data for automation scripts
- **Documentation**: Create examples with realistic data

## Error Handling

The tool provides clear error messages for:
- Invalid data types (shows available options)
- Invalid parameter combinations
- Range validation for numeric inputs
- Missing required parameters