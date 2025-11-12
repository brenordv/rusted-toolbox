# Rusted Toolbox 🦀
This is a collection of free command-line tools I made in an attempt to learn Rust.

We currently have the following tools:
1. A tool to read the [public info on JWT tokens](crates/tool-jwt/readme.md);
2. A high-performance tool to [read messages from EventHub](crates/tool-eventhub-read/readme.md), 
3. and another is to [export the messages](crates/tool-eventhub-export/readme.md);
4. A [CSV data normalizer](crates/tool-csvn/readme.md) tool;
5. A tool that [splits large files](crates/tool-split/readme.md) (including CSV) into smaller ones;
6. A tool [that searches for multiple terms](crates/tool-get-lines/readme.md) inside a text file and creates one output file per search term;
7. A tool that mimics the [cat](crates/tool-cat/readme.md) command from Unix (useful on Windows);
8. A tool that mimics the [touch](crates/tool-touch/readme.md) command from Unix (also useful on Windows);
9. A tool that [generates GUID](crates/tool-guid/readme.md) (uuidv4) in the terminal with some nice options;
10. A tool that [converts unix timestamp](crates/tool-timestamp/readme.md) to readable format and vice versa;
11. A lightweight [HTTP server](crates/tool-http-server/readme.md) for serving static files during development;
12. A [mock data generator](crates/tool-mock/readme.md) for creating test data with various types of realistic information;
13. An [AI-powered chatbot](crates/ai-tool-chatbot/readme.md) agent for local or cloud LLMs;
14. A CLI tool called [HOW](crates/ai-tool-how/readme.md) that fixes broken commands and suggests commands from natural language;
15. A bare-bones, fully private, encrypted P2P chat tool called [Whisper](crates/tool-whisper/readme.md); 
16. A tool called [Gitignore](crates/tool-gitignore/readme.md) that generates/updates the `.gitignore` file of your project automatically; 
17. An image editor called [IMGx](crates/tool-image/readme.md) that allows do quickly do common operations like resizing, converting to another format, and to greyscale;
18. An [MQTT cli tool](crates/tool-mqtt/readme.md) that can be used to quickly send or receive messages from a specific topic.
19. A tool called [aiignore](crates/tool-aiignore/readme.md) that, similarly to gitignore, generates/updates known ai-ignore files for your project automatically;
20. A tool to generate [QRCodes](crates/tool-qrcode/readme.md) that, as the name suggests, can be used to generate QRCodes to file or just printing them to the terminal.
21. A tool to [lookup](crates/tool-lookup/readme.md) that can either find text in multiple files or find files where the filename contains a specific text/pattern/regex.
22. A tool that is like ping, but with extra features. It is called [pingx](crates/tool-pingx/readme.md).
23. A wrapper tool called [Whurl](crates/tool-whurl/readme.md), that allows referencing one hurl file in another.
24. A drop-in replacement for base64 called [b64](crates/tool-b64/readme.md) that comes with a few extra features.

## Ok, but why?
Well, three main reasons:
- **1st**: I have a few tools and helpers made using Go, like my [Azure Eventhub Tools](https://github.com/brenordv/azure-eventhub-tools),
the [go Whisper](https://github.com/brenordv/go-whisper), [gitignore](https://github.com/brenordv/gitignore), and the [go help tools](https://github.com/brenordv/go-help).

While I love Golang, it had a few incidents where malicious software was sneakily added to legit packages:
1. https://thehackernews.com/2025/02/malicious-go-package-exploits-module.html
2. https://thehackernews.com/2025/03/seven-malicious-go-packages-found.html

So I decided to recreate them using Rust. I'm not abandoning Go or saying we shouldn't use it or anything like that. 
I still love Go, I'm using those incidents as an opportunity to improve my knowledge in another great programming 
language and centralizing my tools and helpers in one place.

- **2nd**: I use a couple of tools that are spread around a bunch of repositories, and that's a bit annoying to set up on new
machines. So I also ported the [JWT decoder tool](https://github.com/brenordv/python-snippets/tree/master/jwt_decoder_cli) 
that I created using Python, and the Split tool is an evolution of a powershell script I wrote a long time ago in a blog
post.

- **3rd**: Nice to have all the tools in a single repository, and being to generate a cross-platform executable, which
helps a lot when you have to use Linux, MacOS, and Windows machines frequently.

## Demos
### AI Chatbot
![AI Chatbot Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/ai-chat-bot-demo-0001.mp4)

### Whisper
![Whisper Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/whisper-demo-0001.mp4)

### How
![How Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/how-demo-0001.mp4)

### HTTP Server
![HTTP Server Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/http-server-demo-0001.mp4)

### QR Code
![QR Code Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/qrcode-demo-0001.mp4)

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
### Convenience Scripts
You can use the convenience scripts which will:
1. Install Rust if you don't already have it;
2. Clone the repo;
3. Build the tools;
4. Make the tools available globally for the current user;

Running this script again will update the tools (but not Rust).

#### Convenience build for Ubuntu
Convenience command:
```bash
curl -sSL https://raw.githubusercontent.com/brenordv/rusted-toolbox/refs/heads/master/convenience-build-ubuntu.sh | bash
```

#### Convenience build for MacOs
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
- **#1**: (EH tools) Change this to avoid needing the full connection string just to export messages.
- **#2**: Some files are way too big. Partly due to the documentation + testing. Some methods should be moved to different files.
- **#3**: Do a better job at showing, in a unified way, the default values for CLI arguments.
- **#4**: Add to all the command lines tools the possibility of changing the log level.

## Building on Linux
TODO: Improve this later
```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev libcurl4-openssl-dev libxml2-dev libclang-dev
```