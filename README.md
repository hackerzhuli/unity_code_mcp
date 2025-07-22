# Unity Code MCP Server

> A **coding-focused** Model Context Protocol (MCP) server that enables AI assistants to write Unity code autonomously, surviving domain reloads and compilation cycles.

## 🎯 Why Unity Code MCP?

**The Problem**: Existing Unity MCP servers fail when Unity is compiling or performing domain reloads, causing AI tool calls to break and interrupting development workflows. Additionally, they often include broad Unity operations (scene editing, package management, asset manipulation) that distract from the core task of code development.

**The Solution**: Unity Code MCP is laser-focused on what AI agents do best - **writing code**. It's specifically designed to handle Unity's compilation cycles gracefully while providing only the essential tools needed for autonomous code development: compilation feedback and test execution.

**🎯 Coding-First Philosophy**: Unlike other Unity MCP servers, we deliberately exclude scene editing, package management, and asset manipulation. This focused approach maximizes AI productivity by concentrating on code development workflows where AI agents excel.

## ✨ Key Features

- **🔄 Compilation-Resilient**: Survives Unity domain reloads and compilation cycles
- **🤖 AI-Optimized**: Streamlined tools designed for efficient AI agent workflows
- **⚡ Performance-Focused**: Minimal token usage with targeted, essential operations
- **🧪 Test-Driven**: Comprehensive test execution and reporting capabilities

## 🛠️ Core Capabilities

Unity Code MCP provides **exactly two tools** designed for autonomous code development:

### 1. **Asset Database Refresh**
- Triggers Unity compilation and asset processing
- Returns detailed compilation errors and warnings
- Handles domain reload scenarios gracefully
- Essential for validating code correctness

### 2. **Test Execution**
- Runs Unity tests with comprehensive reporting
- Provides detailed logs and stack traces for failures
- Supports both EditMode and PlayMode tests
- Critical for verifying code functionality

### 🚫 What We Don't Include (By Design)

Unlike other Unity MCP servers, we intentionally **exclude**:
- Scene editing and manipulation
- Package management operations
- Asset creation and modification
- GameObject hierarchy management
- Inspector property editing

### Why Only Two Tools?

This laser-focused approach maximizes AI efficiency by:
- **Eliminating token waste** on non-coding operations
- **Concentrating on AI strengths** - code generation and debugging
- **Providing only essential feedback** for the code development loop
- **Reducing complexity** and potential failure points

Result: AI agents maintain a highly productive development loop: *write → compile → fix errors → test → fix bugs → repeat*.

## 🚀 Quick Start Example

Once configured, AI assistants can interact with Unity like this:

```
AI: I'll create a new player controller script and test it.

1. [AI writes PlayerController.cs]
2. [AI calls: refresh_asset_database]
   → Result: "Compilation successful, no errors"
3. [AI writes PlayerControllerTests.cs]
4. [AI calls: run_tests]
   → Result: "3 tests passed, 1 test failed: NullReferenceException in MovePlayer_ShouldUpdatePosition"
5. [AI fixes the bug and repeats until all tests pass]
```

This seamless workflow allows AI assistants to develop Unity code autonomously, handling compilation and testing without human intervention.

## 📦 Installation

### Prerequisites
- Unity 6.0 or higher
- Rust toolchain (for building from source)
- CMake (required for building dependencies)

### Step 1: Install Unity Package
Install the [Visual Studio Code Editor](https://github.com/hackerzhuli/com.hackerzhuli.code) package in your Unity project.

### Step 2: Get the Binary
**Option A: Download Release** (Recommended)
- Download the latest binary from the [Releases](https://github.com/your-repo/unity-code-mcp/releases) page

**Option B: Build from Source**
```bash
git clone https://github.com/your-repo/unity-code-mcp.git
cd unity-code-mcp
cargo build --release
```

### Step 3: Configure Your AI Assistant
Add the MCP server to your AI assistant configuration:

**For Claude Desktop:**
```json
{
  "mcpServers": {
    "unity-code": {
      "command": "/path/to/unity-code-mcp-server",
      "env": {
        "UNITY_PROJECT_PATH": "/absolute/path/to/your/unity/project"
      }
    }
  }
}
```

**For VS Code with MCP Extension:**
```json
{
  "mcp.servers": {
    "unity-code": {
      "command": "/path/to/unity-code-mcp-server",
      "args": [],
      "env": {
        "UNITY_PROJECT_PATH": "/absolute/path/to/your/unity/project"
      }
    }
  }
}
```

> **Important**: Use absolute paths for both the binary and Unity project directory.

## 🧪 Development & Testing

### Running Tests
To run the test suite:

1. **Start Unity Editor** with the embedded test project:
   ```bash
   # Open Unity Editor and load the project at:
   # ./UnityProject
   ```

2. **Run tests** (single-threaded to avoid Unity conflicts):
   ```bash
   cargo test -- --test-threads=1
   ```

> **Note**: Tests require a running Unity Editor instance with the embedded project loaded. Tests may take 30-60 seconds to complete due to Unity Editor interactions.

### Building from Source

#### Prerequisites
- **CMake**: Required for building the `aws-lc-rs` dependency
  - Windows: Follow the [official guide](https://aws.github.io/aws-lc-rs/requirements/windows.html)
  - macOS: `brew install cmake`
  - Linux: `sudo apt-get install cmake` (Ubuntu/Debian)

#### Build Commands
```bash
# Debug build
cargo build

# Release build (recommended for production)
cargo build --release

# Run with logging
RUST_LOG=debug cargo run
```

## 🤝 Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Run tests with `cargo test -- --test-threads=1`
4. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- [Visual Studio Code Editor for Unity](https://github.com/hackerzhuli/com.hackerzhuli.code) - Required Unity package
- [Model Context Protocol](https://modelcontextprotocol.io/) - The protocol specification
- [Unity Code Pro VS Code Extension](https://github.com/your-repo/unity-code-pro) - VS Code extension that is related

