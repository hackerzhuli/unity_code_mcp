# Unity Code MCP Server

> A **coding-focused** Model Context Protocol (MCP) server that enables AI agents to write Unity code autonomously, surviving domain reloads and compilation cycles.

## 🎯 Why Unity Code MCP?

Unity Code MCP is laser-focused on coding. It's can handle Unity's compilation cycles gracefully while providing only the essential tools needed for autonomous code development: compilation feedback and test execution.

Other Unity MCP servers typically fail when Unity is compiling or performing domain reloads, causing AI tool calls to report an error and AI agents confused about what is going on.

This enables AI agents to develop Unity code autonomously, handling compilation and testing without human intervention.

## ✨ Key Features

- **🤖 Coding-Optimized**: Streamlined tools designed for efficient AI agent coding workflows
- **⚡ Performance-Focused**: Minimal token usage with essential operations
- **🧪 Test-Driven**: Comprehensive test execution and reporting capabilities
- **📦 Self-Contained**: Single binary with no runtime dependencies (no Node.js, Python, or .NET required)
- **🚀 High Performance**: Built in Rust for speed and minimal resource usage

## 🛠️ MCP Server Tools

Unity Code MCP provides **exactly two tools** designed for autonomous code development:

### 1. **Asset Database Refresh**
- Triggers Unity compilation and asset processing
- Returns only compilation errors and other non compile warning and errors during the refresh
- Handles domain reload gracefully

### 2. **Test Execution**
- Runs Unity tests with comprehensive reporting
- Provides detailed stack traces and logs for failures
- Supports both EditMode and PlayMode tests

## 🚀 Autonomous AI agent workflow example

Once configured, AI agents can develop Unity code completely autonomously:

```
User: "Create a player controller that moves with WASD keys and has jump functionality. Make sure it's fully tested."

AI: I'll create a complete player controller with movement and jumping, then write comprehensive tests.

1. [AI writes PlayerController.cs with movement logic]
2. [AI calls: refresh_asset_database]
   → Result: "Compilation failed: CS0246: The type or namespace name 'Rigidbody' could not be found"
3. [AI fixes the missing using UnityEngine; statement]
4. [AI calls: refresh_asset_database]
   → Result: "Compilation successful, no errors"
5. [AI writes PlayerControllerTests.cs with movement and jump tests]
6. [AI calls: run_tests]
   → Result: "5 tests passed, 2 tests failed: NullReferenceException in Jump_ShouldApplyUpwardForce, ArgumentException in Move_WithInvalidInput"
7. [AI analyzes failures and fixes null reference and input validation bugs]
8. [AI calls: refresh_asset_database]
   → Result: "Compilation successful, no errors"
9. [AI calls: run_tests]
   → Result: "All 7 tests passed successfully"

AI: ✅ Complete! Created a fully functional PlayerController with WASD movement, jump mechanics, and comprehensive test coverage. All tests are passing.
```

This demonstrates true autonomous development - from user request to fully tested, working Unity code without any human intervention.

## 📦 Installation

### Prerequisites
- Unity 6.0 or higher
- Rust toolchain (for building from source)
- CMake (required for building dependencies)

### Step 1: Install Unity Package
Install the [Visual Studio Code Editor](https://github.com/hackerzhuli/com.hackerzhuli.code) package in your Unity project.

### Step 2: Get the Binary
**Option A: Download Release** (Recommended) (Windows Only)
- Download the latest binary from the [Releases](https://github.com/your-repo/unity-code-mcp/releases) page

**Option B: Build from Source**
```bash
git clone https://github.com/your-repo/unity-code-mcp.git
cd unity-code-mcp
cargo build --release
```

### Step 3: Configure Your AI Assistant
Add the MCP server to your AI assistant configuration:

**For Claude Desktop/Cursor/Trae:**
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

## Platform support
The code is cross platform, but I can't build or test for other platforms, because I only use Windows. If there are platform specific bugs, you have to fix them yourself.

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
- **C Compiler**: Required for building `aws-lc-rs` dependency
  - Windows: MSVC (Visual Studio Build Tools)
  - macOS: Xcode Command Line Tools (`xcode-select --install`)
  - Linux: GCC (`sudo apt-get install build-essential` on Ubuntu/Debian)
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

