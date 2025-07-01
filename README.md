# Rusted Toolbox 🦀
This is a collection of free command-line tools I made in an attempt to learn Rust.

We currently have the following tools:
1. A tool to read the [public info on JWT tokens](#-jwt---readme);
2. A high-performance tool to [read messages from EventHub](#-eh-read---readme), 
3. and another is to [export the messages](#-eh-export---readme);
4. A [CSV data normalizer](#-csvn---readme) tool;
5. A tool that [splits large files](#-split---readme) (including CSV) into smaller ones;
6. A tool [that searches for multiple terms](#-get-lines---readme) inside a text file and creates one output file per search term;
7. A tool that mimics the [cat](#-cat---readme) command from Unix (useful on Windows);
8. A tool that mimics the [touch](#-touch---readme) command from Unix (also useful on Windows);
9. A tool that [generates GUID](#-guid---readme) (uuidv4) in the terminal with some nice options;
10. A tool that [converts unix timestamp](#-ts---readme) to readable format and vice versa;

## Well, well, well. What do we have here?
### 🐱 cat - [readme](src/tools/cat/readme.md)
Mimics the classic Unix `cat` command. It concatenates files and displays them with optional line numbering, 
character visualization, and formatting features.

**Example:**
```bash
# Show a file with line numbers and visible tabs/line endings
cat -nA config.txt
```
```
     1	server_host=localhost^I# Main server$
     2	port=8080$
     3	$
     4	debug=true$
```

### 📊 csvn - [readme](src/tools/csvn/readme.md)
I hate dealing with CSV files with data missing and having to write a script (or search for something) to fix it, so I
created this tool: A CSV data Normalizer.
This tool fills in empty fields with default values you specify, making your data clean and consistent.

**Example:**
```bash
# Fill missing names with "Unknown" and missing ages with "0"
csvn --file messy_data.csv --value-map "name=Unknown" --value-map "age=0"
```

### 📡 eh-read - [readme](src/tools/eh_read/readme.md)
EventHub Reader - connects to Azure EventHub, reads messages, and stores them locally with checkpoint/resume support. 
This is your gateway to capturing streaming data for later analysis. Performs reasonably well, and during my tests were
able to read about 380 messages/second (it could probably be faster in better filesystems than NTFS).

**Example:**
```bash
# Read from all partitions and export filtered messages to files
eh_read --connection-string "Endpoint=sb://..." --entity-path "events" --read-to-file --dump-filter "ERROR"
```

### ☁️ eh-export - [readme](src/tools/eh_export/readme.md)
One of the features of the aforementioned `EventHub Reader` is the ability to read messages from Eventhub and save them
to a local embedded database. After that, you need a way to export those messages from the DB to files.
Enter EventHub Export tool! It exports messages from local databases (created by eh_read) to various file formats. 

You don't need to use this tool, since you can read the messages directly to files, but with this, you can read at 
max speed and then export the messages.

**Cool example:**
```bash
# Export all temperature sensor messages to JSON with metadata
eh_export --config export-config.json --export-format json --dump-filter "temperature" --include-metadata
```

### 🔍 get-lines - [readme](src/tools/get_lines/readme.md)
High-performance text search (case insensitive) utility that extracts lines containing specific patterns. 
It's like grep but with some neat features like separate output files per search term and parallel processing.

**Example:**
```bash
# Search for errors and warnings, output to separate files with 4 workers
get_lines --file server.log --search "error,warning,critical" --output results --workers 4
```
Creates `results/error.txt`, `results/warning.txt`, and `results/critical.txt` files automatically.

### 🆔 guid - [readme](src/tools/guid/readme.md)
GUID generator with some extra features that I find useful, like continuous generation at intervals and clipboard
integration or automatically copying the GUID to the clipboard.

**Example:**
```bash
# Generate a new GUID every 2 seconds (great for testing)
guid --continuous-generation 2.0
```
Output:
```plaintext
🚦 Press Ctrl+C to stop...
550e8400-e29b-41d4-a716-446655440000
```
(The GUID values will be printed over and over on the same line.

### 🔐 jwt - [readme](src/tools/jwt/readme.md)
JWT decoder that extracts and displays token contents without signature verification. 
Handy for debugging authentication issues and understanding what's in your tokens.

**Example:**
```bash
# Decode a JWT and copy the user ID to clipboard
jwt --copy-to-clipboard client_id "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
```
Instantly see what's inside those mysterious JWT tokens, and copies the value of `client_id` claim to the clipboard, if
it exists.

### ✂️ split - [readme](src/tools/split/readme.md)
File splitter that handles both regular text files and CSV files with header preservation. 

**Example:**
```bash
# Split a large CSV into 1000-line chunks, keeping headers in each file
split --file huge_dataset.csv --csv-mode --lines-per-file 1000
```
Each output file gets the original headers: no more manual header management!

### 👆 touch - [readme](src/tools/touch/readme.md)
My implementation of the Unix `touch` command for updating file timestamps. 
Creates files if they don't exist and handles various timestamp formats.

**Cool example:**
```bash
# Set specific timestamp on multiple files
touch -d "2024-12-25 15:30:00" holiday_file1.txt holiday_file2.txt
```

### ⏰ ts - [readme](src/tools/ts/readme.md)
This tool is a simple bidirectional timestamp converter, that converts between Unix timestamps and human-readable dates
automatically. It detects what you give it and converts to the other format. Not a perfect port of the Unix tool `date`,
but it works similarly.
NGL, I created this because I usually want to know what the value of `_ts` field (from Azure Cosmos Db) mean. Now it's
easy and fast.

**Cool example:**
```bash
# Convert Unix timestamp to readable date
ts 1703764800
```
Output:
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: 1703764800

UTC Time: 2023-12-28T12:00:00Z
Local Time: 2023-12-28T13:00:00+0100
```

## Ok, but why?
Well, three main reasons:
1st: I have a few tools and helpers made using Go, like my [Azure Eventhub Tools](https://github.com/brenordv/azure-eventhub-tools),
and the [go help tools](https://github.com/brenordv/go-help).

While I love Golang, it had a few incidents where malicious software was sneakily added to legit packages:
1. https://thehackernews.com/2025/02/malicious-go-package-exploits-module.html
2. https://thehackernews.com/2025/03/seven-malicious-go-packages-found.html

So I decided to recreate them using Rust. I'm not abandoning Go or saying we shouldn't use it or anything like that. 
I still love Go, I'm using those incidents as an opportunity to improve my knowledge in another great programming 
language and centralizing my tools and helpers in one place.

2nd: I use a couple of tools that are spread around a bunch of repositories, and that's a bit annoying to set up on new
machines. So I also ported the [JWT decoder tool](https://github.com/brenordv/python-snippets/tree/master/jwt_decoder_cli) 
that I created using Python, and the Split tool is an evolution of a powershell script I wrote a long time ago in a blog
post.

3rd: Nice to have all the tools being cross-platform, centralized in a single repository, and each compiled to a single
executable file.

## Installation

### Building all tools locally
Considering you have Rust installed, you can build all tools by running:

**On Windows:**
```terminal
build.bat
```

**On Linux/MacOs:**
```bash
chmod +x ./build.sh
./build.sh
```

### Convenience build for MacOs
If you're on a MacOs, you can use the `convenience-build-macos.sh` which will:
1. Install Rust (using Brew) if you don't already have it;
2. Clone the repo;
3. Build the tools;
4. Make the tools available globally for the current user;

Running this script again will update the tools (but not Rust).

Convenience command:
```bash
curl -sSL https://raw.githubusercontent.com/brenordv/rusted-toolbox/refs/heads/master/convenience-build-macos.sh | bash
```

## Contributing
By the time I'm writing this, we have about 8.2 billion people in the world. Being optimistic, this means that the 
chances of someone wanting to contribute (or maybe even use the tools here) are about `1:8,200,000,000` (that one 
person being me).

Even so, I've created the [contributing readme](CONTRIBUTING.md) so future-me can remember how to organize things
when I come back to this project after a while. 

## License
Everything under [GNU Public License V3](LICENSE.md). 

TL;DR:
1. Anyone can copy, modify, and distribute this software.
2. You have to include the license and copyright notice with every distribution.
3. You can use this software privately.
4. You can use this software for commercial purposes.
5. If you dare to build your business solely from this code, you risk open-sourcing the whole code base.
6. If you modify it, you have to indicate changes made to the code.
7. Any modifications of this code base MUST be distributed with the same license, GPLv3.
8. This software is provided without a warranty.
9. The software author or license cannot be held liable for any damage inflicted by the software.

## Todo
- **#1**: Change this to avoid needing the full connection string just to export messages.
- **#2**: Some files are way too big. Partly due to the documentation + testing. Some methods should be moved to different files.
- **#3**: Do a better job at showing the default values for CLI arguments + do it in a unified way.
- **#4**: Unify all the constants in one file. We don't have that many to justify different files.