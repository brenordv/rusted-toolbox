# AI Chatbot Tool

An interactive AI-powered command-line chatbot that supports multiple AI platforms including OpenAI, local models via 
OpenWebUI (if you have enough hardware), and OpenRouter. The tool provides a conversational interface with customizable
AI personalities and maintains conversation history.

## What It Does

The AI Chatbot tool creates an interactive chat session between you and an AI assistant with the following features:

- **Multi-Platform Support**: Works with OpenAI, local LLMs via OpenWebUI, and OpenRouter
- **Custom Personalities**: Load different AI personalities from text files to customize behavior
- **Conversation History**: Maintains context throughout the chat session
- **Request Logging**: Automatically logs all API requests and responses for debugging
- **Environment Configuration**: Flexible setup via environment variables

## Prerequisites
Before using the chatbot, you need to:

1. **Set up AI Platform**: Configure one of the supported AI platforms (OpenAI, local OpenWebUI, or OpenRouter)
2. **Create Personality Files**: Prepare text files containing system prompts for different AI personalities
3. **Configure Environment**: Set the required environment variables for your chosen platform

## Environment Variables
### Required Variables
```bash
# AI Platform selection (required)
AI_PLATFORM=openai|local|openrouter

# Personality configuration (required)
AI_CHAT_PERSONALITIES_FOLDER=/path/to/personality/files
```

### OpenAI Configuration
```bash
AI_PLATFORM=openai
OPENAI_API_KEY=your_openai_api_key
OPENAI_MODEL=gpt-4  # or your preferred model
OPENAI_TEMPERATURE=0.7  # optional, defaults to 1.0
OPENAI_REQUEST_HISTORY_PATH=/path/to/logs  # optional
```

### Local OpenWebUI Configuration
```bash
AI_PLATFORM=local
LOCAL_OPENWEBUI_API_KEY=your_local_api_key
LOCAL_OPENWEBUI_MODEL=your_local_model_name
LOCAL_OPENWEBUI_URL=http://localhost:11434/v1/chat/completions
LOCAL_OPENWEBUI_TEMPERATURE=0.7  # optional
LOCAL_OPENWEBUI_REQUEST_HISTORY_PATH=/path/to/logs  # optional
```

### OpenRouter Configuration
```bash
AI_PLATFORM=openrouter
OPENROUTER_API_KEY=your_openrouter_api_key
OPENROUTER_MODEL=your_preferred_model
OPENROUTER_TEMPERATURE=0.7  # optional
OPENROUTER_REQUEST_HISTORY_PATH=/path/to/logs  # optional
```

### Optional Variables
```bash
# User identification (optional - will prompt if not set)
AI_CHAT_USER_NAME=YourName

# Initial message to send to AI when starting (optional)
AI_CHAT_INITIAL_MSG_TO_AI="Hello! I'd like to start our conversation."
```

## Personality Files

Create text files in your personalities folder containing system prompts that define the AI's behavior:

**Example: `/personalities/helpful_assistant.txt`**
```
You are a helpful and knowledgeable assistant. You provide clear, accurate, and helpful responses to user questions. You are friendly but professional, and you always try to be as helpful as possible while being concise.
```

**Example: `/personalities/creative_writer.txt`**
```
You are a creative writing assistant. You help users with storytelling, character development, plot ideas, and creative writing techniques. You are imaginative, inspiring, and always encourage creativity while providing practical writing advice.
```

The tool will automatically extract the AI name from the filename (e.g., "helpful_assistant.txt" becomes "helpful assistant").

## Usage Examples

### Basic Usage
```bash
# Start the chatbot (will prompt for configuration if not set via env vars)
ai-chatbot
```

### With Environment File
```bash
# Create a .env file with your configuration
echo "AI_PLATFORM=openai" >> .env
echo "OPENAI_API_KEY=your_key_here" >> .env
echo "AI_CHAT_PERSONALITIES_FOLDER=./personalities" >> .env

# Run the chatbot
ai-chatbot
```

### Sample Conversation Flow
```
💬 ChatBot v1.0
---------------------------
🧍 User: John
🤖 Ai: helpful assistant

John    > Hello! Can you help me understand how neural networks work?

helpful assistant > Hello John! I'd be happy to explain neural networks...

John    > That's fascinating! Can you give me a simple example?

helpful assistant > Certainly! Let me walk you through a simple example...
```

## Features

### Interactive Chat Interface
- Clean, formatted conversation display
- Role-based message printing with consistent alignment
- Real-time user input handling
- Graceful exit handling (Ctrl+C)

### AI Platform Flexibility
- **OpenAI**: Full support for GPT models with organization settings
- **Local Models**: Connect to local LLMs via OpenWebUI-compatible APIs
- **OpenRouter**: Access to multiple AI models through OpenRouter

### Conversation Management
- Maintains full conversation history throughout the session (but not between sessions)
- System message injection for personality consistency
- Context preservation across multiple exchanges
- Request/response logging for debugging and analysis

### Personality System
- File-based personality management
- Interactive personality selection at startup
- Flexible system prompt configuration

## Request Logging
All API interactions are automatically logged to help with:
- Debugging API issues
- Monitoring usage and costs
- Analyzing conversation patterns
- Troubleshooting response quality

Log files include:
- Request timestamps
- Full request payloads
- Response status codes
- Complete response bodies