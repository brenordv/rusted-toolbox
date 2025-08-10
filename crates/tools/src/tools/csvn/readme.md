# CSV Normalizer (csvn)

## What it does

The CSV Normalizer (`csvn`) processes CSV files by filling empty or missing fields with predefined default values. 
It creates a new normalized version of your CSV file with a `_normalized` suffix, ensuring data consistency and 
preventing errors in downstream processing systems that cannot handle missing values.

**Key Features:**
- Fills empty/missing CSV fields with default values
- Auto-detects headers or accepts custom headers
- Real-time progress feedback with processing speed
- Graceful shutdown with data preservation
- Optional string cleaning (removes non-printable characters)
- High-performance buffered I/O (128KB buffer)
- Memory-optimized string interning for repeated values

## Command-Line Options
- `-f, --file`: Input CSV file path (required)
- `-e, --headers`: Comma-separated headers (optional, auto-detected if not provided)
- `-i, --feedback-interval`: Progress update interval in rows (default: 100)
- `-c, --clean-string`: Enable string cleaning (warning: significantly slows processing)
- `-v, --value-map`: Key=Value pairs for default values (required, repeatable)
  - Use `*` as key for universal default value
  - Use specific column names for targeted defaults
  - Multiple mappings: `--value-map "name=Unknown" --value-map "age=0"`

## Examples
### Basic Usage - Universal Default Value
**Command:**
```bash
csvn --file data.csv --value-map "*=N/A"
```

**Input (data.csv):**
```csv
name,age,city
John,25,
Alice,,New York
,30,London
```

**Output (data_normalized.csv):**
```csv
name,age,city
John,25,N/A
Alice,N/A,New York
N/A,30,London
```

### Column-Specific Default Values

**Command:**
```bash
csvn --file sales.csv --value-map "name=Unknown" --value-map "amount=0" --value-map "status=Pending"
```

**Input (sales.csv):**
```csv
name,amount,status
John,100,
,250,Complete
Alice,,
```

**Output (sales_normalized.csv):**
```csv
name,amount,status
John,100,Pending
Unknown,250,Complete
Alice,0,Pending
```

### Custom Headers with Progress Feedback

**Command:**
```bash
csvn --file data.csv --headers "product,price,quantity" --value-map "price=0.00" --value-map "quantity=1" --feedback-interval 50
```

**Input (data.csv):**
```csv
Laptop,999.99,
Mouse,,5
,25.50,
```

**Output (data_normalized.csv):**
```csv
product,price,quantity
Laptop,999.99,1
Mouse,0.00,5
Unknown,25.50,1
```

### With String Cleaning

**Command:**
```bash
csvn --file messy.csv --value-map "*=Clean" --clean-string
```

**Input (messy.csv):**
```csv
name,notes
John,"Good\x00customer"
,Special\tclient
```

**Output (messy_normalized.csv):**
```csv
name,notes
John,Goodcustomer
Clean,Specialclient
```

## Real-World Use Cases

- **Data Science & Analytics**: Normalizing datasets to prevent ML models from failing due to missing values 
- **Software Development**: Preparing CSV imports for databases with NOT NULL constraints 
- **QA Engineer**: Creating complete test datasets from partial CSV files
- **ETL Developer**: Standardizing source data before loading into data warehouses
- **API Developer**: Normalizing CSV uploads before processing user data

## Known Issues
1. **Silent Malformed CSV Handling**: The tool silently skips malformed CSV lines without logging which specific lines
were problematic. This is a design choice to focus on performance of execution, and to keep the code simpler.

2. **Missing Data Type Validation**: The tool doesn't validate that default values match expected data types for 
columns. In this case, we are trusting the user. What could go wrong?

3. **Limited Error Context**: When default values cannot be found for specific columns, the tool prints warnings to 
stderr but continues processing, potentially resulting in incomplete normalization without clear indication of how many
records were affected.