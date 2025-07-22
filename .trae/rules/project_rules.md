# Unity Code MCP Server - Project Rules

## Project Purpose

This project aims to create a reliable Model Context Protocol (MCP) server that enables AI assistants to communicate effectively with Unity Editor. The primary motivation is to address the limitations of existing Unity MCP servers, particularly their inability to handle Unity's domain reload functionality, which causes Unity Editor components to become unavailable.

### Core Objectives
- Build a robust MCP server that survives Unity domain reloads
- Provide essential Unity integration features:
  - Retrieve Unity logs
  - Execute and manage Unity tests
- Serve as a component for the Unity Code Pro VS Code extension
- Enable seamless AI-Unity Editor communication

## Tech Stack

### Core Technologies
- **Language**: Rust
- **MCP Framework**: `rust-mcp-sdk` (core dependency)
- **Async Runtime**: Tokio (implicit dependency via rust-mcp-sdk)
- **Architecture**: Single-threaded async to minimize complexity

### Dependencies
- `rust-mcp-sdk`: Primary MCP implementation
- `tokio`: Async I/O runtime

## Development Guidelines

### Architecture Principles
1. **Simplicity First**: Use single-threaded async architecture to avoid unnecessary complexity

### Code Standards
1. **Rust Best Practices**:
   - Follow Rust naming conventions
   - Maintain comprehensive error handling

2. **Async Programming**:
   - Leverage Tokio's single-threaded runtime
   - Use async/await patterns consistently
   - Avoid blocking operations in async contexts

3. **MCP Protocol Compliance**:
   - Implement MCP tools according to specification
   - Ensure proper error handling and response formatting
   - Maintain protocol version compatibility

### Project Structure
- Keep the codebase minimal and focused
- Organize code by functionality (logs, tests, core MCP handling)
- Maintain clear separation between Unity communication and MCP protocol handling

### Testing Strategy
- Unit tests for internal functionality (modules that doesn't require a running Unity Editor instance to test)
- A real (but minimal) Unity project is embedded in `UnityProject` for running related tests (that doesn't require a running Unity Editor instance, just need files, including Unity generated files)
- Manual tests with Unity Editor scenarios
- Don't create examples binaries(for testing or any other reason), only do unit tests for testing
- Run test sequentially with `cargo test -- --test-threads=1`, otherwise, Unity Editor related tests can fail.

### Compatibility
- Target stable Rust versions
- Ensure compatibility with Unity 6.0 or higher
- Maintain cross-platform compatibility (Windows, macOS, Linux)

## Development Workflow

1. **Setup**: Use `cargo` for dependency management and building
2. **Testing**: Run tests before commits with `cargo test`
3. **Formatting**: Use `cargo fmt` before commits
4. **Linting**: Address `cargo clippy` warnings
5. **Documentation**: Update docs for any API changes

## Integration Notes

- This server will be integrated into the Unity Code Pro VS Code extension
- Users should be able to configure VS Code (or forks) to use this MCP server
- Design APIs with VS Code extension consumption in mind
- Consider configuration and setup simplicity for end users