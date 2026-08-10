# QR Code Generator Tool

The QR Code Generator is a versatile command-line utility that creates QR codes for text data and WiFi credentials.
The tool supports multiple output formats including console display, PNG images, and SVG files, making it perfect for 
quick sharing of information, WiFi credentials, URLs, or any text data that needs to be easily scannable.

**Key Features:**
- **Text QR Codes**: Generate QR codes for any text, URLs, or data strings
- **WiFi QR Codes**: Create WiFi credential QR codes for easy network sharing
- **Multiple Output Formats**: Console display, PNG images, and SVG vector graphics
- **Automatic Error Correction**: Intelligent error correction level selection based on data size
- **Smart File Naming**: Automatic timestamp-based filename generation
- **Quality Optimization**: High-quality output with configurable scaling and quiet zones

## Command-Line Options

### Data Input Options
- `-t, --text <TEXT>`: Text payload for QR code (URLs, messages, data, etc.)
- `-s, --wifi-ssid <SSID>`: WiFi network name for WiFi credential QR codes
- `-p, --wifi-password <PASSWORD>`: WiFi network password 
- `-a, --wifi-auth <AUTH>`: WiFi authentication type (default: WPA)

### Output Options
- `-o, --output-file <FILENAME>`: Custom output filename (auto-generates if not specified)
- `-f, --output-format <FORMAT>`: Output file format (png, svg)
- `-x, --dont-print`: Skip printing QR code to console
- `-n, --no-header`: Suppress runtime information header

## Examples

### Basic Text QR Code
**Command:**
```bash
qrcode --text "Hello, World!"
```
**Output:**
```
QrCode Generator v1.0.0
---------------------------
Generating Text QR code
Text: Hello, World

  ██████████████  ██████  ██      ██  ██████████████  
  ██          ██  ██  ██    ████████  ██          ██  
  ██  ██████  ██  ██████  ██  ████    ██  ██████  ██  
  ██  ██████  ██    ██                ██  ██████  ██  
  ██  ██████  ██    ██  ██  ████      ██  ██████  ██  
  ██          ██  ██  ████████  ██    ██          ██  
  ██████████████  ██  ██  ██  ██  ██  ██████████████  
                  ████  ████    ██                    
      ██████  ██  ██████  ████  ██  ██████    ██████  
  ████  ██████      ██    ██  ████████  ██    ██      
      ██      ██████    ██████    ██  ████████  ████  
  ████████      ████  ██████  ████      ████    ████  
              ████        ██████  ██████  ██████████  
  ██████  ██          ██        ██      ██    ██      
  ██  ██████  ██  ████    ██  ████████  ██████  ████  
  ██    ██  ██    ████████████  ██    ████        ██  
  ██    ██  ██████  ██████████  ████████████████      
                  ██  ██        ████      ██  ██  ██  
  ██████████████      ████    ██████  ██  ██  ██████  
  ██          ██    ██  ██    ██  ██      ████    ██  
  ██  ██████  ██  ████            ████████████████    
  ██  ██████  ██  ████████    ████████████    ██  ██  
  ██  ██████  ██  ██████          ██    ██  ██    ██  
  ██          ██          ████████  ████    ██    ██  
  ██████████████        ██████  ██    ██    ████████  
```

### URL QR Code with File Output
**Command:**
```bash
qrcode --text "https://github.com/brenordv/rusted-toolbox" --output-format png
```
**Output:** Creates `qrcode--2024-09-25-14-30-45.png` with the QR code

### WiFi Credential QR Code
**Command:**
```bash
qrcode --wifi-ssid "MyNetwork" --wifi-password "SecurePass123" --wifi-auth WPA2
```
**Output:**
```
QrCode Generator v1.0.0
---------------------------
Generating Wifi QR code
SSID: MyNetwork
Password: SecurePass123
Auth: WPA2

[Console QR Code Display]
```

### SVG Output with Custom Filename
**Command:**
```bash
qrcode --text "Contact: john@example.com" --output-format svg --output-file contact-info
```
**Output:** Creates `contact-info.svg` with scalable vector QR code

### Silent Generation (No Console Output)
**Command:**
```bash
qrcode --text "Quick data transfer" --output-format png --dont-print --no-header
```
**Output:** Silently creates PNG file without console display

### WiFi QR Code for Guest Network
**Command:**
```bash
qrcode --wifi-ssid "GuestNetwork" --wifi-password "Welcome2024" --output-format svg --output-file guest-wifi
```
**Output:** Creates `guest-wifi.svg` with WiFi credentials that can be easily shared

## WiFi QR Code Format

The tool generates WiFi QR codes following the standard format:
```
WIFI:T:<auth_type>;S:<ssid>;P:<password>;;
```

**Supported Authentication Types:**
- `WPA` - WPA Personal (default)
- `WPA2` - WPA2 Personal  
- `WEP` - WEP (not recommended)
- `nopass` - Open network (no password)

## Output Formats

### Console Display
- Uses Unicode block characters (██) for high-contrast display
- Includes 1-module quiet zone border for proper scanning
- Automatically displayed unless `--dont-print` is specified

### PNG Images
- High-quality raster images with 30x scaling factor
- 2-module quiet zone for optimal scanning reliability
- White background with black modules
- Suitable for printing and digital sharing

### SVG Vector Graphics
- Scalable vector format perfect for any size requirements
- Crisp edges at any resolution
- Small file size ideal for web use
- Can be easily embedded in documents or websites

## Error Correction Levels

The tool automatically selects optimal error correction based on data size:

| Data Length   | Error Correction | Recovery Capability  |
|---------------|------------------|----------------------|
| 0-50 chars    | High (~30%)      | Maximum reliability  |
| 51-100 chars  | Quartile (~25%)  | Good reliability     |
| 101-300 chars | Medium (~15%)    | Standard reliability |
| 300+ chars    | Low (~7%)        | Basic reliability    |

## File Naming Convention

When no output filename is specified, the tool automatically generates descriptive names:
```
qrcode--<timestamp>.<format>
```

**Example:** `qrcode--2024-09-25-14-30-45.png`

## Known Issues

1. **Large Data**: Very large text inputs may result in dense QR codes that are difficult to scan
2. **WiFi Compatibility**: Some older devices may not support WiFi QR code automatic connection
3. **Console Display**: Unicode block characters may not display correctly in all terminal environments
4. **SVG Scaling**: SVG files have fixed viewBox dimensions and may need CSS scaling for web use