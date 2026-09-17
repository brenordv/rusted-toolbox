# JWT Tool

A command-line JWT (JSON Web Token) decoder that extracts and displays public claims from JWT tokens without signature
verification. 

This tool is designed for inspecting, debugging, and analyzing public data in JWT tokens.

## What It Does

The JWT tool decodes JWT tokens and extracts their public claims, headers, and payload information. 
It disables signature verification to focus on extracting publicly available data, making it useful for:

- Inspecting JWT token contents during development
- Debugging authentication issues
- Security analysis and auditing
- Educational purposes to understand JWT structure
- Quick token validation and expiration checking

## Command Line Examples

### Basic Usage - Pretty Print (Default)
```bash
jwt "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
```
**Output:**
```
No expiration claim
----Claims:------------
sub: 1234567890
name: John Doe
iat: 1516239022
```

### Show the Tool Header
The header block only prints when `--app-header` is passed:
```bash
jwt --app-header "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
```
**Output:**
```
jwt (2.0.2)
---------------------------------------------------
- Basic Runtime Config
  - Verbose mode: <unused>
  - Log level: Warning
  - Log to stdout: false
  - Log to file: false
  - Rotate log file by day: false
- Tool Runtime Config
  - Token length: 155
  - Print format: Pretty

No expiration claim
----Claims:------------
sub: 1234567890
name: John Doe
iat: 1516239022
```

### CSV Output Format
```bash
jwt --print csv "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
```
**Output:**
```
iat,name,sub
1516239022,John Doe,1234567890
```

### JSON Output Format
```bash
jwt -p json "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
```
**Output:**
```json
{
  "sub": "1234567890",
  "name": "John Doe",
  "iat": 1516239022
}
```

### Copy Claim to Clipboard
```bash
jwt --copy-to-clipboard sub "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
```
**Output:** Copies "1234567890" to clipboard

### Handle Bearer Token Format
```bash
jwt "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
```
**Note:** The tool automatically removes "Bearer" prefix and whitespace